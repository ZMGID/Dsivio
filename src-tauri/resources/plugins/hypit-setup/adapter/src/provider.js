import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { delimiter, extname, join } from "node:path";

import { EndpointServiceError, canonicalize, defineEndpointPackage, wakeAfter } from "@hypit/hypit/endpoint-kit";
import { generationTypes, sealGeneratedImageSet, sealGeneratedVideoSet } from "@hypit/hypit/generation";

import { IMAGE_MODELS, PORT_TABLES, VIDEO_MODELS, fulfilments, portsFor } from "./models.js";

export const providerModule = { name: "@dsivio/hypit-provider", version: "1" };

const EXTENSIONS = new Map([
  ["image/png", ".png"],
  ["image/jpeg", ".jpg"],
  ["image/webp", ".webp"],
  ["video/mp4", ".mp4"],
  ["video/webm", ".webm"],
  ["video/quicktime", ".mov"],
  ["audio/mpeg", ".mp3"],
  ["audio/mp3", ".mp3"],
  ["audio/wav", ".wav"],
  ["audio/x-wav", ".wav"],
]);
const MEDIA_TYPES = new Map([...EXTENSIONS].map(([mediaType, extension]) => [extension, mediaType]));

/** What differs between the two result kinds; everything else is shared. */
const KINDS = {
  image: { returns: generationTypes.imageSet, seal: sealGeneratedImageSet, resultKey: "images" },
  video: { returns: generationTypes.videoSet, seal: sealGeneratedVideoSet, resultKey: "videos" },
};

/** Refuse, before anything is submitted, what Dsivio's own rules for this model would refuse. */
export function checkLimits(offer, ports) {
  const c = offer.model.capabilities;
  if (c === null || c === undefined) return { status: "supported" };
  const one = (name) => scalar(ports[name]);
  const count = (...names) => names.reduce((sum, name) => sum + (ports[name]?.length ?? 0), 0);
  const refuse = (reason) => ({ status: "unsupported", reason: `${offer.model.id}: ${reason}` });
  const inList = (value, list) => value === undefined || !Array.isArray(list) || list.length === 0 || list.map(String).includes(String(value));
  if (one("webSearch") === true) return refuse("web search is not available through Dsivio; set web-search to false");
  if (offer.kind === "video") {
    if (!inList(one("duration"), c.durations)) return refuse(`duration ${one("duration")}s is not offered (allowed: ${c.durations.join(", ")})`);
    if (!inList(one("resolution"), c.resolutions)) return refuse(`resolution ${one("resolution")} is not offered (allowed: ${c.resolutions.join(", ")})`);
    if (one("aspectRatio") !== "auto" && !inList(one("aspectRatio"), c.ratios)) return refuse(`aspect ratio ${one("aspectRatio")} is not offered (allowed: ${c.ratios.join(", ")})`);
    if (c.lastFrameNeedsFirst !== false && count("lastFrame") > 0 && count("firstFrame") === 0) return refuse("a last frame needs a first frame");
    const references = count("referenceImage", "referenceVideo", "referenceAudio", "images");
    if (c.framesExcludeReferences && references > 0 && count("firstFrame", "lastFrame") > 0) return refuse("first/last frames cannot be combined with reference media");
    if ((ports.referenceImage?.length ?? 0) > c.maxReferenceImages || (ports.images?.length ?? 0) > c.maxReferenceImages) return refuse(`at most ${c.maxReferenceImages} reference images`);
    if ((ports.referenceVideo?.length ?? 0) > c.maxReferenceVideos) return refuse(`at most ${c.maxReferenceVideos} reference videos`);
    if ((ports.referenceAudio?.length ?? 0) > c.maxReferenceAudios) return refuse(`at most ${c.maxReferenceAudios} reference audios`);
    if (c.referenceAudioNeedsVisual && count("referenceAudio") > 0 && count("referenceImage", "referenceVideo") === 0) return refuse("reference audio needs a reference image or video with it");
    if (c.maxPromptLength && String(one("prompt") ?? "").length > c.maxPromptLength) return refuse(`the prompt is longer than ${c.maxPromptLength} characters`);
  } else {
    if ((ports.images?.length ?? 0) > c.maxReferenceImages) return refuse(`at most ${c.maxReferenceImages} reference images`);
    if (one("resolution") !== "auto" && !inList(one("resolution"), c.sizes)) return refuse(`size ${one("resolution")} is not offered (allowed: ${c.sizes.join(", ")})`);
    if (one("aspectRatio") !== "auto" && !inList(one("aspectRatio"), c.ratios)) return refuse(`aspect ratio ${one("aspectRatio")} is not offered (allowed: ${c.ratios.join(", ")})`);
  }
  return { status: "supported" };
}

function scalar(values) {
  return values === undefined || values.length === 0 ? undefined : values[0];
}

function nonemptyText(value, subject) {
  if (typeof value !== "string" || value.trim().length === 0) throw new Error(`Dsivio ${subject} must be nonempty text`);
  return value;
}

function lastLine(text) {
  const lines = text.split("\n").map((line) => line.trim()).filter((line) => line.length > 0);
  return lines.length === 0 ? undefined : lines[lines.length - 1];
}

/** Dsivio accepts letters, digits, `-`, `_`, `.`, `:` only, in at most 128 characters. */
export function idempotencyKey(id) {
  const cleaned = String(id).replace(/[^A-Za-z0-9._:-]/gu, "-");
  const bounded = cleaned.length <= 120
    ? cleaned
    : `${cleaned.slice(0, 12)}-${createHash("sha256").update(cleaned).digest("hex").slice(0, 40)}`;
  return `hypit-${bounded}`;
}

function describe(error) {
  return error instanceof Error ? error.message : String(error);
}

/** The Dsivio command line, with the launcher Dsivio installs at startup as a fallback. */
export function resolveCommand(declared) {
  const command = declared ?? "dsivio";
  if (command.includes("/") || command.includes("\\")) return command;
  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    if (directory.length === 0) continue;
    const candidate = join(directory, command);
    if (existsSync(candidate)) return candidate;
  }
  const installed = join(homedir(), ".kivio", "bin", process.platform === "win32" ? "dsivio.cmd" : "dsivio");
  return existsSync(installed) ? installed : command;
}

function runCommand(file, args, input, timeoutMs) {
  return new Promise((resolve, reject) => {
    // Windows launches the `.cmd` shim through the shell; arguments still travel as an array.
    const child = spawn(file, args, { stdio: ["pipe", "pipe", "pipe"], windowsHide: true, shell: /\.cmd$/iu.test(file) });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(new EndpointServiceError("DSIVIO_UNAVAILABLE", `Cannot run ${file}: ${describe(error)}`));
    });
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new EndpointServiceError("DSIVIO_TIMEOUT", `${file} ${args.slice(0, 2).join(" ")} did not answer within ${timeoutMs} ms`));
    }, timeoutMs);
    timer.unref?.();
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code: code ?? -1, stdout, stderr });
    });
    child.stdin.end(input ?? "");
  });
}

function parseJson(stdout) {
  const line = lastLine(stdout);
  if (line === undefined) return undefined;
  try {
    const value = JSON.parse(line);
    return value !== null && typeof value === "object" ? value : undefined;
  } catch {
    return undefined;
  }
}

/** Exit codes are the contract of `dsivio media` (see references/dsivio.md). */
const FAILURE_CODES = new Map([
  [2, "DSIVIO_REQUEST_REFUSED"],
  [3, "DSIVIO_REQUEST_REFUSED"],
  [4, "DSIVIO_GENERATION_FAILED"],
  [5, "DSIVIO_SUBMISSION_UNCERTAIN"],
  [6, "DSIVIO_NOT_RUNNING"],
]);

function commandFailure(outcome, args, json) {
  const detail = typeof json?.error === "string"
    ? json.error
    : typeof json?.error?.message === "string"
      ? json.error.message
      : lastLine(outcome.stderr) ?? "no diagnostic";
  const subject = `Dsivio ${args.slice(0, 2).join(" ")} exited ${outcome.code}`;
  const error = new EndpointServiceError(
    FAILURE_CODES.get(outcome.code) ?? "DSIVIO_FAILED",
    outcome.code === 6 ? `${subject}: Dsivio is not running; open Dsivio and try again` : `${subject}: ${detail}`,
  );
  error.exitCode = outcome.code;
  error.json = json;
  return error;
}

function outputOf(task) {
  const outputs = Array.isArray(task.outputs) ? task.outputs : [];
  const first = outputs.find((entry) => typeof entry?.path === "string");
  return first === undefined
    ? undefined
    : { path: first.path, ...(typeof first.mime === "string" ? { mime: first.mime } : {}) };
}

function taskFailure(task, id) {
  const detail = typeof task.error === "string"
    ? task.error
    : typeof task.error?.message === "string"
      ? task.error.message
      : "no diagnostic";
  // A failed task that can still be resumed kept a receipt the provider stopped returning: it may
  // have been charged, so it is reported apart from an ordinary generation failure.
  if (task.canResume === true) {
    return { code: "DSIVIO_RECEIPT_LOST", message: `Dsivio task ${id} (receipt ${task.remoteId ?? "unknown"}): ${detail}. Check with the provider before generating again; \`dsivio media status ${id} --resume\` queries the same receipt.` };
  }
  return { code: "DSIVIO_GENERATION_FAILED", message: `Dsivio task ${id} failed: ${detail}` };
}

async function discard(directory) {
  if (typeof directory !== "string" || directory.length === 0) return;
  await rm(directory, { recursive: true, force: true }).catch(() => {});
}

/** Flags for one request, from the ports Hypit sent and the table of the row that serves it. */
export function argumentsFor(kind, row, request, tmpFiles) {
  const table = PORT_TABLES[kind];
  const args = [];
  for (const [name, values] of Object.entries(request.ports ?? {})) {
    const port = table[name];
    if (port === undefined || name === "prompt" || values === undefined || values.length === 0) continue;
    if (port.kind === "off-only") continue;
    // `auto` video ratio means "the model's default"; the image command accepts `auto` itself.
    if (port.kind === "flag" && kind === "video" && name === "aspectRatio" && values[0] === "auto") continue;
    if (port.kind === "flag") args.push(port.flag, String(values[0]));
    else if (port.kind === "switch") {
      if (values[0] === true) args.push(port.flag);
      else if (values[0] === false) args.push("--options-json", JSON.stringify({ generateAudio: false }));
    } else if (port.kind === "media") {
      for (const item of values) args.push(port.flag, tmpFiles.get(item));
    }
  }
  return args;
}

/**
 * The Dsivio models this endpoint serves, read once from `dsivio media models` when Hypit activates
 * it. `supports` is synchronous in Hypit, so it answers from this snapshot; a model changed in Dsivio
 * later is picked up after `hypit runtime down` / `hypit runtime up`.
 */
export function offersFrom(listed, options = {}) {
  const offers = [];
  for (const kind of ["image", "video"]) {
    const pin = kind === "image" ? options.imageModel : options.videoModel;
    const candidates = pin?.trim() ? listed.filter((entry) => entry.kind === kind && entry.id === pin) : listed;
    const found = fulfilments(kind, candidates);
    if (pin?.trim() && found.length === 0) {
      const reason = candidates.length === 0
        ? `Pinned ${kind} model ${pin} is not enabled in Dsivio Settings > 媒体创作`
        : `Pinned ${kind} model ${pin} has no compatible Hypit model`;
      offers.push({ ...placeholderOffers(reason).find((offer) => offer.kind === kind), diagnosticCode: "DSIVIO_MODEL_PIN_INVALID" });
    }
    for (const { row, model } of found) {
      offers.push({ kind, row, model, ports: portsFor(kind, row, model) });
    }
  }
  return offers;
}

/**
 * Hypit refuses to load an endpoint that declares no capability, which would hide the real cause
 * (Dsivio not running yet, or no model enabled). When nothing can be offered, declare the first
 * candidate of each kind with no model behind it: every request for it is refused with the reason,
 * and `hypit doctor` says what to do. Once Dsivio answers, the next start declares the real ones.
 */
export function placeholderOffers(reason) {
  return ["image", "video"].map((kind) => {
    const row = (kind === "image" ? IMAGE_MODELS : VIDEO_MODELS)[0];
    return { kind, row, model: { id: "(no model)", kind, capabilities: null }, ports: [], unavailable: reason };
  });
}

export function offerKey(hypit) {
  return `${hypit.module}#${hypit.name}`;
}

/**
 * `options.run(args, stdin)` executes the Dsivio command and returns its parsed JSON (tests replace it);
 * `options.offers` is the snapshot from `offersFrom`. Nothing here talks to a service or holds a key.
 */
export function createDsivioProvider(options) {
  const file = resolveCommand(options.command);
  const interval = options.pollIntervalMs ?? 5_000;
  const timeoutMs = options.requestTimeoutMs ?? 120_000;
  const concurrency = options.concurrency ?? 2;
  const offers = options.offers ?? [];

  const media = options.run ?? (async (args, input) => {
    const outcome = await runCommand(file, args, input, timeoutMs);
    const json = parseJson(outcome.stdout);
    if (outcome.code !== 0) throw commandFailure(outcome, args, json);
    if (json === undefined) throw new Error(`Dsivio ${args.slice(0, 2).join(" ")} printed no JSON result`);
    return json;
  });

  function unsupported(offer, request) {
    if (offer.unavailable !== undefined) return { status: "unsupported", reason: offer.unavailable };
    const ports = request.constraints?.ports ?? {};
    const foreign = Object.keys(ports).filter((name) => !offer.ports.includes(name));
    for (const slot of request.pendingInputs ?? []) {
      if (!offer.ports.includes(slot.input)) foreign.push(slot.input);
    }
    if (foreign.length > 0) {
      return {
        status: "unsupported",
        reason: `${offer.model.id} in Dsivio does not accept ${[...new Set(foreign)].join(", ")}; choose another model in Dsivio Settings > 媒体创作`,
      };
    }
    return checkLimits(offer, request.constraints?.ports ?? {});
  }

  function cap(offer) {
    const kind = KINDS[offer.kind];
    const endpoint = {
      kind: offer.kind,
      async start(context) {
        const request = context.need.constraints ?? { ports: {} };
        const prompt = nonemptyText(scalar(request.ports.prompt), "prompt");
        const directory = await mkdtemp(join(tmpdir(), "hypit-dsivio-"));
        try {
          // Every media port carries Artifacts; each becomes a temporary file the command can read.
          const files = new Map();
          let counter = 0;
          for (const [name, values] of Object.entries(request.ports)) {
            if (PORT_TABLES[offer.kind][name]?.kind !== "media") continue;
            for (const item of values ?? []) {
              const artifact = item?.artifact;
              if (artifact === undefined) throw new Error(`A ${name} input carries no artifact`);
              const extension = EXTENSIONS.get(artifact.mediaType);
              if (name === "referenceAudio" && extension !== ".mp3" && extension !== ".wav") {
                throw new EndpointServiceError("DSIVIO_REQUEST_REFUSED", `Unsupported ${name} media type ${artifact.mediaType}; use WAV or MP3`);
              }
              const bytes = await context.resources.get(artifact.resource);
              if (bytes === undefined) throw new Error(`The ${name} input is unavailable`);
              const path = join(directory, `input-${++counter}${extension ?? ".bin"}`);
              await writeFile(path, bytes);
              files.set(item, path);
            }
          }
          const args = [
            "media", offer.kind, "--no-wait", "--source", "hypit",
            "--idempotency-key", idempotencyKey(context.operation ?? context.command.id),
            "--prompt-file", "-",
            ...argumentsFor(offer.kind, offer.row, request, files),
            "--model", offer.model.id,
          ];
          await context.reportProgress?.({ phase: `Submitting ${offer.kind} request to Dsivio (${offer.model.id})` });
          let submitted;
          try {
            submitted = await media(args, prompt);
          } catch (error) {
            // Exit 5 (submission uncertain) may still name a task; keep it as evidence and never resubmit.
            if (typeof error.json?.id === "string") {
              await context.checkpoint?.({ handle: { id: error.json.id }, receipt: { id: error.json.id } });
            }
            throw error;
          }
          const id = nonemptyText(submitted.id, "task id");
          await context.checkpoint?.({ handle: { id }, receipt: { id } });
          if (submitted.status === "failed") return { status: "failed", receipt: { id }, failure: taskFailure(submitted, id) };
          const output = outputOf(submitted);
          return output === undefined
            ? { ...wakeAfter({ id }, interval), receipt: { id } }
            : { status: "ready", handle: { id, path: output.path, mediaType: output.mime }, receipt: { id } };
        } finally {
          // Inputs are copies for this submission only; the result lives in Dsivio's task folder.
          await discard(directory);
        }
      },
      async poll(context) {
        const id = nonemptyText(context.handle?.id, "task id");
        let task;
        try {
          task = await media(["media", "status", id]);
        } catch (error) {
          // The paid task lives in Dsivio and continues when it is opened again; failing here would
          // make Hypit give up on a task that is still running.
          if (error?.exitCode === 6 || error?.code === "DSIVIO_NOT_RUNNING") {
            return wakeAfter({ id }, interval, Date.now(), { phase: `Dsivio is not running; open Dsivio to continue task ${id}` });
          }
          throw error;
        }
        if (task.status === "running") {
          return wakeAfter({ id }, interval, Date.now(), { phase: task.error || `Dsivio task ${id} is running` });
        }
        if (task.status === "failed") return { status: "failed", receipt: { id }, failure: taskFailure(task, id) };
        if (task.status !== "succeeded") {
          throw new EndpointServiceError("DSIVIO_FAILED", `Dsivio task ${id} reports the unknown state ${String(task.status)}`);
        }
        const output = outputOf(task);
        if (output === undefined) {
          return { status: "failed", receipt: { id }, failure: {
            code: "DSIVIO_RESULT_MISSING",
            message: `Dsivio task ${id} succeeded without an output file`,
          } };
        }
        return { status: "ready", handle: { id, path: output.path, mediaType: output.mime } };
      },
      async collect(context) {
        const handle = context.handle ?? {};
        const path = nonemptyText(handle.path, "output path");
        const mediaType = typeof handle.mediaType === "string" ? handle.mediaType : MEDIA_TYPES.get(extname(path).toLowerCase());
        if (mediaType === undefined) throw new Error(`Dsivio output ${path} has an unknown media type`);
        await context.reportProgress?.({ phase: `Collecting generated ${offer.kind}` });
        const bytes = await readFile(path);
        const artifact = await context.resources.put(new Uint8Array(bytes), mediaType);
        return { status: "completed", result: { value: {
          kind: "inline",
          value: canonicalize(kind.seal({ [kind.resultKey]: [artifact] })),
        } } };
      },
    };
    return {
      capability: { module: { name: offer.row.hypit.module, version: "1" }, name: offer.row.hypit.name },
      returns: kind.returns,
      lifecycle: "asynchronous",
      supports: (request) => unsupported(offer, request),
      endpoint,
    };
  }

  return defineEndpointPackage({
    module: providerModule,
    facet: "media",
    instance: options.instance,
    pool: options.pool,
    defaultConcurrency: concurrency,
    actionLimits: { submit: { concurrency }, poll: { concurrency: 4 }, collect: { concurrency: 1 } },
    // Dsivio bills the model account the user enabled in Dsivio; this Endpoint holds no account.
    pricing: { kind: "local" },
    capabilities: offers.map(cap),
  });
}

/** `dsivio media models`, or an empty list when Dsivio cannot answer (not running, no models): the
 *  endpoint then offers nothing and `hypit doctor` says why, instead of failing to load. */
export async function listModels(options, run) {
  const file = resolveCommand(options.command);
  try {
    if (run !== undefined) return await run(["media", "models"]);
    const outcome = await runCommand(file, ["media", "models"], undefined, 60_000);
    const json = parseJson(outcome.stdout);
    if (outcome.code !== 0) throw commandFailure(outcome, ["media", "models"], json);
    return Array.isArray(json) ? json : [];
  } catch {
    return [];
  }
}

/** What `hypit doctor` shows: whether Dsivio answers, which Hypit models it can serve, and what is left out. */
export async function probeDsivio(options, offers) {
  const file = resolveCommand(options.command);
  let listed;
  try {
    const outcome = await runCommand(file, ["media", "models"], undefined, 60_000);
    const json = parseJson(outcome.stdout);
    if (outcome.code !== 0) throw commandFailure(outcome, ["media", "models"], json);
    listed = Array.isArray(json) ? json : [];
  } catch (error) {
    return [{ severity: "error", code: "DSIVIO_UNAVAILABLE", message: describe(error) }];
  }
  const kinds = new Set(listed.map((entry) => entry?.kind));
  const missing = ["image", "video"].filter((kind) => !kinds.has(kind));
  const currentOffers = offers ?? offersFrom(listed, options);
  const pinDiagnostics = currentOffers.filter((offer) => offer.diagnosticCode !== undefined)
    .map((offer) => ({ severity: "error", code: offer.diagnosticCode, message: offer.unavailable }));
  if (missing.length > 0) {
    return [{
      severity: "error",
      code: "DSIVIO_NO_MODEL",
      message: `Dsivio has no ${missing.join(" and ")} model enabled; enable one in Dsivio Settings > 媒体创作`,
    }, ...pinDiagnostics];
  }
  const served = currentOffers.filter((offer) => offer.unavailable === undefined);
  const servedIds = new Set(served.map((offer) => offer.model.id));
  const diagnostics = [{
    severity: "info",
    code: "DSIVIO_MEDIA_READY",
    message: `Dsivio media ready at ${file}: ${served.map((offer) => `${offer.row.hypit.name} <- ${offer.model.id}`).join("; ") || "no Hypit model matches"}`,
  }];
  diagnostics.push(...pinDiagnostics);
  for (const entry of listed) {
    // A pin (or lower priority) can exclude a compatible model without making it unsupported.
    if (!servedIds.has(entry.id) && fulfilments(entry.kind, [entry]).length === 0) {
      diagnostics.push({
        severity: "warning",
        code: "DSIVIO_MODEL_NOT_IN_HYPIT",
        message: `${entry.id} is enabled in Dsivio but Hypit has no model it can stand in for, so Hypit cannot use it`,
      });
    }
  }
  for (const offer of served) {
    if (offer.model.capabilities === null || offer.model.capabilities === undefined) {
      diagnostics.push({
        severity: "warning",
        code: "DSIVIO_MODEL_UNKNOWN",
        message: `Dsivio has no facts about ${offer.model.id}; Hypit sends only the prompt to it`,
      });
    }
  }
  return diagnostics;
}
