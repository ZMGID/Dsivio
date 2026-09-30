// Runs with `node --test`. The @hypit imports are replaced by a tiny loader so the adapter's own logic
// (model mapping, flags, limits, idempotency, exit codes, polling) is tested without installing Hypit.
// `hypit-ports.json` records the real port tables of the Hypit models the adapter maps to.
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { register } from "node:module";

register("./hypit-stub-loader.mjs", import.meta.url);
const { createDsivioProvider, idempotencyKey, resolveCommand, offersFrom, placeholderOffers, argumentsFor, checkLimits, probeDsivio } = await import("../src/provider.js");
const { VIDEO_MODELS, IMAGE_MODELS, PORT_TABLES, fulfilments, portsFor } = await import("../src/models.js");
const { default: activation, readDsivioConfig } = await import("../src/activation.js");
const hypitPorts = JSON.parse(await readFile(new URL("./hypit-ports.json", import.meta.url), "utf8"));

// Shaped like `dsivio media models` output (capabilities come from Dsivio's own catalog).
const video = (model, capabilities, extra = {}) => ({ kind: "video", id: `p/${model}`, model, providerId: "p", default: false, known: capabilities !== null, capabilities, ...extra });
const image = (model, capabilities) => ({ kind: "image", id: `p/${model}`, model, providerId: "p", default: false, known: capabilities !== null, capabilities });
const SEEDANCE = { protocol: "seedance", modes: ["text", "image", "frames", "reference"], durations: [4, 5, 6, 15], resolutions: ["480p", "720p", "1080p"], ratios: ["16:9", "9:16", "adaptive"], audioToggle: true, firstFrame: true, lastFrame: true, maxReferenceImages: 9, maxReferenceVideos: 3, maxReferenceAudios: 3, referenceAudioNeedsVisual: true, localReferenceMedia: false, framesExcludeReferences: true, maxPromptLength: null, defaults: {} };
const GROK = { protocol: "xai_video", modes: ["text", "image", "reference"], durations: [1, 2, 5, 15], resolutions: ["480p", "720p", "1080p"], ratios: ["16:9", "9:16", "1:1"], audioToggle: true, firstFrame: true, lastFrame: false, maxReferenceImages: 7, maxReferenceVideos: 0, maxReferenceAudios: 0, referenceAudioNeedsVisual: false, localReferenceMedia: false, framesExcludeReferences: true, maxPromptLength: null, defaults: {} };
const H3 = { ...SEEDANCE, protocol: "minimax_h3", audioToggle: false, localReferenceMedia: true, maxReferenceImages: 9 };
const GPT_IMAGE = { maxReferenceImages: 16, sizes: ["512", "1K", "2K", "4K"], ratios: ["1:1", "16:9", "9:16"], maxCount: 4 };

const models = [
  video("doubao-seedance-2-5-260628", SEEDANCE),
  video("grok-imagine-video-1.5", GROK),
  video("MiniMax-H3", H3),
  video("veo-3.1-generate-preview", { ...GROK, protocol: "veo" }),
  video("my-private-video", null),
  image("gpt-image-2.5-flare", GPT_IMAGE),
  image("gemini-3.1-flash-image", GPT_IMAGE),
];

/** A scripted `dsivio` : answers each call from a queue and records the calls. */
function scripted(replies) {
  const calls = [];
  const run = async (args, input) => {
    calls.push({ args, input });
    const reply = replies.shift();
    if (reply instanceof Error) throw reply;
    return reply;
  };
  return { calls, run };
}

function providerFor(listed, run, extra = {}) {
  return createDsivioProvider({ instance: "d", pool: "d", offers: offersFrom(listed, extra), run, ...extra });
}
const capabilityOf = (provider, name) => {
  const found = provider.capabilities.find((entry) => entry.capability.name === name);
  assert.ok(found, `capability ${name} is offered`);
  return found;
};
const names = (provider) => provider.capabilities.map((entry) => entry.capability.name).sort();

function startContext(constraints, extra = {}) {
  const checkpoints = [];
  return {
    checkpoints,
    context: {
      command: { id: "cmd:1" },
      operation: "op_1/2",
      need: { constraints },
      resources: { get: async () => new Uint8Array([1, 2, 3]), put: async (bytes, mediaType) => ({ resource: "r1", mediaType, size: bytes.length }) },
      checkpoint: async (value) => { checkpoints.push(value); },
      reportProgress: async () => {},
      ...extra,
    },
  };
}
const ref = (mediaType = "image/png") => ({ artifact: { resource: "a", mediaType } });

test("the mapping only names ports that Hypit's real models have, and covers what Hypit requires of them", () => {
  for (const row of [...VIDEO_MODELS, ...IMAGE_MODELS]) {
    const real = hypitPorts[row.hypit.name];
    assert.ok(real, `${row.hypit.name} exists in Hypit`);
    for (const port of row.ports) assert.ok(real.ports.includes(port), `${row.hypit.name} has a ${port} port`);
    // Every port Hypit requires must be mapped, or a request for this model could never be built.
    for (const required of real.required) assert.ok(row.ports.includes(required), `${row.hypit.name} maps its required ${required}`);
    // A row must know how to carry every port it lists.
    const kind = real.result === "image" ? "image" : "video";
    for (const port of row.ports) assert.ok(PORT_TABLES[kind][port], `${row.hypit.name}: ${port} has a command line`);
  }
});

test("Hypit models needing ports Dsivio cannot carry are left out rather than half supported", () => {
  const image = hypitPorts["seedream-5-lite"];
  assert.ok(image.required.some((port) => !["prompt", "aspectRatio", "images"].includes(port)));
  assert.ok(!IMAGE_MODELS.some((row) => row.hypit.name === "seedream-5-lite" || row.hypit.name.startsWith("nano-banana")));
});

test("which Hypit models are offered follows the models enabled in Dsivio, in Dsivio's order", () => {
  const provider = providerFor(models, async () => ({}));
  assert.deepEqual(names(provider), ["gpt-image-2", "grok-imagine-video-1.5-preview", "minimax-h3", "seedance-2.5"]);
  // Nothing enabled means nothing offered, not a stale fixed pair.
  assert.equal(offersFrom([]).length, 0);
  // Dsivio lists the priority order: the first matching model serves a Hypit model.
  const both = [image("gpt-image-a", GPT_IMAGE), image("gpt-image-b", GPT_IMAGE)];
  assert.equal(offersFrom(both)[0].model.model, "gpt-image-a");
});

test("configuration carries no service address or key", () => {
  assert.throws(() => readDsivioConfig("i", { baseUrl: "https://x", apiKey: "k" }), /Unknown/u);
  assert.equal(readDsivioConfig("i", {}).imageModel, undefined);
});

test("first and last frames are offered exactly when the Dsivio model can carry them", () => {
  const offered = (model) => portsFor("video", VIDEO_MODELS.find((row) => row.match.test(model.model)), model);
  const seedance = offered(models[0]);
  assert.ok(seedance.includes("firstFrame") && seedance.includes("lastFrame") && seedance.includes("generateAudio"));
  const grok = offered(models[1]);
  assert.ok(!grok.includes("firstFrame") || GROK.firstFrame);
  assert.ok(!grok.includes("lastFrame"));
  const noFrames = offered(video("doubao-seedance-2-5-260628", { ...SEEDANCE, firstFrame: false, lastFrame: false }));
  assert.ok(!noFrames.includes("firstFrame") && !noFrames.includes("lastFrame"));
  // A model Dsivio has no facts about carries the prompt only.
  assert.deepEqual(portsFor("video", VIDEO_MODELS[0], video("doubao-seedance-2-5-260628", null)), ["prompt"]);
});

test("a request for something the chosen Dsivio model cannot do is refused with the model and the reason", () => {
  const provider = providerFor(models, async () => ({}));
  const grok = capabilityOf(provider, "grok-imagine-video-1.5-preview");
  const base = { prompt: ["p"], aspectRatio: ["16:9"], resolution: ["720p"], duration: [5] };
  assert.equal(grok.supports({ constraints: { ports: base } }).status, "supported");
  const tooMany = grok.supports({ constraints: { ports: { ...base, images: Array.from({ length: 8 }, ref) } } });
  assert.equal(tooMany.status, "unsupported");
  assert.match(tooMany.reason, /grok-imagine-video-1\.5.*at most 7 reference images/u);
  assert.match(grok.supports({ constraints: { ports: { ...base, duration: [40] } } }).reason, /duration 40s is not offered/u);
  assert.match(grok.supports({ constraints: { ports: { ...base, resolution: ["4k"] } } }).reason, /resolution 4k/u);
  const seedance = capabilityOf(provider, "seedance-2.5");
  const s = { prompt: ["p"], aspectRatio: ["16:9"], resolution: ["720p"], duration: [5], generateAudio: [false], webSearch: [false] };
  assert.equal(seedance.supports({ constraints: { ports: { ...s, firstFrame: [ref()], lastFrame: [ref()] } } }).status, "supported");
  assert.match(seedance.supports({ constraints: { ports: { ...s, lastFrame: [ref()] } } }).reason, /needs a first frame/u);
  assert.match(seedance.supports({ constraints: { ports: { ...s, firstFrame: [ref()], referenceImage: [ref()] } } }).reason, /cannot be combined with reference media/u);
  assert.match(seedance.supports({ constraints: { ports: { ...s, referenceAudio: [ref("audio/mp3")] } } }).reason, /reference audio needs a reference image or video/u);
  assert.match(seedance.supports({ constraints: { ports: { ...s, webSearch: [true] } } }).reason, /web search is not available/u);
  // The same request goes through on a model whose Dsivio side accepts it, and nothing is silently dropped.
  assert.equal(seedance.supports({ constraints: { ports: { ...s, referenceImage: [ref()], referenceAudio: [ref("audio/mp3")] } } }).status, "supported");
});

test("checkLimits knows nothing about a model Dsivio has no facts about, so it never invents a refusal", () => {
  assert.equal(checkLimits({ kind: "video", model: { id: "x", capabilities: null } }, { duration: [999] }).status, "supported");
});

test("every Hypit port maps to the flag Dsivio documents, and media inputs become the files given", () => {
  const files = new Map();
  const request = {
    ports: {
      prompt: ["p"], aspectRatio: ["9:16"], resolution: ["1080p"], duration: [8], generateAudio: [true], webSearch: [false],
      firstFrame: [ref()], lastFrame: [ref()], referenceImage: [ref(), ref()], referenceVideo: [ref("video/mp4")], referenceAudio: [ref("audio/mp3")],
    },
  };
  for (const [name, values] of Object.entries(request.ports)) if (PORT_TABLES.video[name]?.kind === "media") for (const item of values) files.set(item, `/tmp/${name}`);
  const args = argumentsFor("video", VIDEO_MODELS[0], request, files);
  const pairs = [];
  for (let i = 0; i < args.length; i += 1) if (args[i].startsWith("--")) pairs.push([args[i], args[i + 1]?.startsWith("--") ? undefined : args[i + 1]]);
  assert.deepEqual(pairs.filter(([flag]) => ["--ratio", "--resolution", "--duration"].includes(flag)), [["--ratio", "9:16"], ["--resolution", "1080p"], ["--duration", "8"]]);
  assert.ok(args.includes("--audio"));
  assert.deepEqual(pairs.filter(([flag]) => ["--first-frame", "--last-frame", "--ref-video", "--ref-audio"].includes(flag)), [["--first-frame", "/tmp/firstFrame"], ["--last-frame", "/tmp/lastFrame"], ["--ref-video", "/tmp/referenceVideo"], ["--ref-audio", "/tmp/referenceAudio"]]);
  assert.equal(pairs.filter(([flag]) => flag === "--ref").length, 2);
  assert.ok(!args.includes("webSearch") && !args.includes("--web-search"));
  const off = argumentsFor("video", VIDEO_MODELS[0], { ports: { prompt: ["p"], generateAudio: [false] } }, new Map());
  assert.ok(!off.includes("--audio"));
});

test("start submits without waiting, sends the model Dsivio chose, pipes the prompt on stdin and records the receipt first", async () => {
  const dsivio = scripted([{ id: "task-1", status: "running", outputs: [] }]);
  const provider = providerFor(models, dsivio.run);
  const { context, checkpoints } = startContext({ ports: { prompt: ["镜头缓慢推进"], aspectRatio: ["16:9"], resolution: ["720p"], duration: [6], images: [ref()] } });
  const outcome = await capabilityOf(provider, "grok-imagine-video-1.5-preview").endpoint.start(context);
  const submit = dsivio.calls[0];
  assert.deepEqual(submit.args.slice(0, 4), ["media", "video", "--no-wait", "--source"]);
  for (const [flag, value] of [["--resolution", "720p"], ["--ratio", "16:9"], ["--duration", "6"], ["--prompt-file", "-"], ["--model", "p/grok-imagine-video-1.5"]]) {
    assert.equal(submit.args[submit.args.indexOf(flag) + 1], value, flag);
  }
  assert.match(submit.args[submit.args.indexOf("--ref") + 1], /input-1\.png$/u);
  assert.equal(submit.input, "镜头缓慢推进");
  assert.ok(!submit.args.includes("镜头缓慢推进"), "the prompt never travels as an argument");
  assert.deepEqual(checkpoints, [{ handle: { id: "task-1" }, receipt: { id: "task-1" } }]);
  assert.equal(outcome.status, "pending");
});

test("first and last frames reach the command for a model that can carry them", async () => {
  const dsivio = scripted([{ id: "task-2", status: "running" }]);
  const provider = providerFor(models, dsivio.run);
  const { context } = startContext({ ports: { prompt: ["p"], aspectRatio: ["16:9"], resolution: ["720p"], duration: [5], generateAudio: [false], webSearch: [false], firstFrame: [ref()], lastFrame: [ref("image/jpeg")] } });
  await capabilityOf(provider, "seedance-2.5").endpoint.start(context);
  const args = dsivio.calls[0].args;
  assert.match(args[args.indexOf("--first-frame") + 1], /input-1\.png$/u);
  assert.match(args[args.indexOf("--last-frame") + 1], /input-2\.jpg$/u);
  assert.equal(args[args.indexOf("--model") + 1], "p/doubao-seedance-2-5-260628");
});

test("a pinned model determines both the offered capability and the submitted model", async () => {
  const dsivio = scripted([{ id: "t", status: "running" }]);
  const pinned = video("doubao-seedance-2-5-260628", { ...SEEDANCE, lastFrame: false, durations: [5] }, { id: "second/doubao-seedance-2-5-260628" });
  const provider = providerFor([...models, pinned], dsivio.run, { videoModel: pinned.id });
  assert.deepEqual(names(provider), ["gpt-image-2", "seedance-2.5"]);
  const capability = capabilityOf(provider, "seedance-2.5");
  assert.match(capability.supports({ constraints: { ports: { prompt: ["p"], lastFrame: [ref()] } } }).reason, /second\/.*lastFrame/u);
  assert.match(capability.supports({ constraints: { ports: { prompt: ["p"], duration: [15] } } }).reason, /duration 15/u);
  const { context } = startContext({ ports: { prompt: ["p"], aspectRatio: ["16:9"], resolution: ["720p"], duration: [5] } });
  await capability.endpoint.start(context);
  assert.equal(dsivio.calls[0].args.at(-1), pinned.id);
});

test("reference audio keeps a supported extension and its bytes until the CLI accepts it", async () => {
  for (const [mime, extension] of [["audio/wav", "wav"], ["audio/x-wav", "wav"], ["audio/mpeg", "mp3"], ["audio/mp3", "mp3"]]) {
    let path;
    const provider = providerFor(models, async (args) => {
      path = args[args.indexOf("--ref-audio") + 1];
      assert.ok(path.endsWith(`.${extension}`), path);
      assert.deepEqual(await readFile(path), Buffer.from([1, 2, 3]));
      return { id: "audio-task", status: "running" };
    });
    const { context } = startContext({ ports: { prompt: ["p"], referenceImage: [ref()], referenceAudio: [ref(mime)] } });
    assert.equal((await capabilityOf(provider, "seedance-2.5").endpoint.start(context)).status, "pending");
    await assert.rejects(readFile(path), { code: "ENOENT" });
  }
});

test("unsupported reference audio fails before any submission", async () => {
  const dsivio = scripted([]);
  const provider = providerFor(models, dsivio.run);
  const { context } = startContext({ ports: { prompt: ["p"], referenceAudio: [ref("audio/ogg")] } });
  await assert.rejects(capabilityOf(provider, "seedance-2.5").endpoint.start(context), /referenceAudio.*audio\/ogg/u);
  assert.equal(dsivio.calls.length, 0);
});

test("both audio switch values reach the CLI explicitly", async () => {
  for (const enabled of [true, false]) {
    const dsivio = scripted([{ id: "audio-task", status: "running" }]);
    const { context } = startContext({ ports: { prompt: ["p"], generateAudio: [enabled] } });
    await capabilityOf(providerFor(models, dsivio.run), "seedance-2.5").endpoint.start(context);
    const args = dsivio.calls[0].args;
    if (enabled) assert.ok(args.includes("--audio"));
    else {
      assert.ok(!args.includes("--audio"));
      assert.deepEqual(JSON.parse(args[args.indexOf("--options-json") + 1]), { generateAudio: false });
    }
  }
});

test("activation diagnoses invalid pins without falling back to another model", async () => {
  const fake = join(await mkdtemp(join(tmpdir(), "fake-dsivio-")), "dsivio");
  await writeFile(fake, `#!/bin/sh\necho '${JSON.stringify(models)}'\n`, { mode: 0o755 });
  for (const videoModel of ["p/not-enabled", "p/veo-3.1-generate-preview", "p/gpt-image-2.5-flare"]) {
    const { endpoint, diagnose } = await activation.hostFacets[0].activate({ instance: "d", config: { command: fake, videoModel } });
    const videos = endpoint.capabilities.filter((entry) => entry.endpoint.kind === "video");
    assert.ok(videos.length > 0);
    for (const entry of videos) {
      const verdict = entry.supports({ constraints: { ports: { prompt: ["p"] } } });
      assert.equal(verdict.status, "unsupported");
      assert.ok(verdict.reason.includes(videoModel));
    }
    assert.ok((await diagnose()).some((item) => item.severity === "error" && item.code === "DSIVIO_MODEL_PIN_INVALID" && item.message.includes(videoModel)));
  }
  const pinned = video("doubao-seedance-2-5-260628", { ...SEEDANCE, lastFrame: false }, { id: "second/seedance" });
  await writeFile(fake, `#!/bin/sh\necho '${JSON.stringify([...models, pinned])}'\n`, { mode: 0o755 });
  const active = await activation.hostFacets[0].activate({ instance: "d", config: { command: fake, videoModel: pinned.id } });
  assert.deepEqual(names(active.endpoint), ["gpt-image-2", "seedance-2.5"]);
  assert.equal(capabilityOf(active.endpoint, "seedance-2.5").supports({ constraints: { ports: { prompt: ["p"], lastFrame: [ref()] } } }).status, "unsupported");
  const diagnostics = await active.diagnose();
  assert.ok(diagnostics.find((item) => item.code === "DSIVIO_MEDIA_READY").message.includes(pinned.id));
  assert.ok(!diagnostics.some((item) => item.code === "DSIVIO_MODEL_NOT_IN_HYPIT" && item.message.startsWith(models[0].id)));
  // Missing a whole media pool must not hide the invalid pin diagnostic.
  await writeFile(fake, `#!/bin/sh\necho '${JSON.stringify(models.filter((model) => model.kind === "image"))}'\n`, { mode: 0o755 });
  const missing = await activation.hostFacets[0].activate({ instance: "d", config: { command: fake, videoModel: pinned.id } });
  assert.deepEqual((await missing.diagnose()).map((item) => item.code), ["DSIVIO_NO_MODEL", "DSIVIO_MODEL_PIN_INVALID"]);
});

test("image pins use the pinned model's reference limits", () => {
  const pinned = image("gpt-image-limited", { ...GPT_IMAGE, maxReferenceImages: 1 });
  const provider = providerFor([...models, pinned], async () => ({}), { imageModel: pinned.id });
  const verdict = capabilityOf(provider, "gpt-image-2").supports({ constraints: { ports: { prompt: ["p"], images: [ref(), ref()] } } });
  assert.equal(verdict.status, "unsupported");
  assert.match(verdict.reason, /gpt-image-limited.*at most 1/u);
});

test("image start uses --size for the tier and passes reference images as --ref", async () => {
  const dsivio = scripted([{ id: "img-1", status: "running" }]);
  const provider = providerFor(models, dsivio.run);
  const { context } = startContext({ ports: { prompt: ["白底主图"], aspectRatio: ["1:1"], resolution: ["2K"], images: [ref("image/jpeg")] } });
  await capabilityOf(provider, "gpt-image-2").endpoint.start(context);
  const args = dsivio.calls[0].args;
  assert.equal(args[args.indexOf("--size") + 1], "2K");
  assert.equal(args[args.indexOf("--model") + 1], "p/gpt-image-2.5-flare");
  assert.match(args[args.indexOf("--ref") + 1], /input-1\.jpg$/u);
});

test("the idempotency key is legal for Dsivio and stable per submission", () => {
  const legal = /^[A-Za-z0-9._:-]{1,128}$/u;
  const long = `need:author%3A%2FUsers%2Fzmmini%2F${"x".repeat(300)}`;
  for (const id of ["op_1/2", "need:author%3A%2FUsers%2Fa.svml%3A%3Acomponent", long, "é/中文 space"]) {
    assert.match(idempotencyKey(id), legal);
    assert.equal(idempotencyKey(id), idempotencyKey(id));
  }
  assert.notEqual(idempotencyKey("op_1"), idempotencyKey("op_2"));
  assert.notEqual(idempotencyKey(`${long}a`), idempotencyKey(`${long}b`));
});

test("an uncertain submission (exit 5) is reported, keeps the receipt it names, and is never retried", async () => {
  const uncertain = Object.assign(new Error("Dsivio media video exited 5: 提交结果不确定"), { code: "DSIVIO_SUBMISSION_UNCERTAIN", exitCode: 5, json: { id: "task-9" } });
  const dsivio = scripted([uncertain]);
  const provider = providerFor(models, dsivio.run);
  const { context, checkpoints } = startContext({ ports: { prompt: ["p"], aspectRatio: ["16:9"], resolution: ["720p"], duration: [5] } });
  await assert.rejects(capabilityOf(provider, "grok-imagine-video-1.5-preview").endpoint.start(context), /exited 5/u);
  assert.equal(dsivio.calls.length, 1, "no second submission");
  assert.deepEqual(checkpoints, [{ handle: { id: "task-9" }, receipt: { id: "task-9" } }]);
});

test("a refused request leaves no receipt and no temporary files behind", async () => {
  const refused = Object.assign(new Error("Dsivio media image exited 3: 服务拒绝"), { exitCode: 3 });
  const dsivio = scripted([refused]);
  const provider = providerFor(models, dsivio.run);
  const { context, checkpoints } = startContext({ ports: { prompt: ["p"], aspectRatio: ["1:1"], resolution: ["1K"], images: [ref()] } });
  await assert.rejects(capabilityOf(provider, "gpt-image-2").endpoint.start(context), /exited 3/u);
  assert.deepEqual(checkpoints, []);
  const file = dsivio.calls[0].args[dsivio.calls[0].args.indexOf("--ref") + 1];
  await assert.rejects(readFile(file), { code: "ENOENT" });
});

test("poll waits while running, fails with Dsivio's error, and hands a finished file to collect", async () => {
  const dir = await mkdtemp(join(tmpdir(), "adapter-test-"));
  const file = join(dir, "output.mp4");
  await writeFile(file, new Uint8Array([0, 0, 0, 24]));
  const dsivio = scripted([
    { id: "t", status: "running", canResume: true, error: "任务已提交，查询暂不可用（HTTP 404），继续等待同一回执" },
    { id: "t", status: "failed", error: "供应商生成失败" },
    { id: "t", status: "succeeded", outputs: [{ path: file, mime: "video/mp4" }] },
    { id: "t", status: "succeeded", outputs: [] },
  ]);
  const provider = providerFor(models, dsivio.run);
  const { endpoint } = capabilityOf(provider, "grok-imagine-video-1.5-preview");
  const base = { handle: { id: "t" }, command: { id: "c" }, operation: "o" };
  assert.equal((await endpoint.poll(base)).status, "pending");
  const failed = await endpoint.poll(base);
  assert.equal(failed.status, "failed");
  assert.match(failed.failure.message, /供应商生成失败/u);
  const ready = await endpoint.poll(base);
  assert.equal(ready.status, "ready");
  assert.equal(dsivio.calls[0].args.join(" "), "media status t");
  const collected = await endpoint.collect({ ...base, handle: ready.handle, resources: { put: async (bytes, mediaType) => ({ resource: "r", mediaType, size: bytes.length }) }, reportProgress: async () => {} });
  assert.equal(collected.status, "completed");
  assert.equal((await endpoint.poll(base)).failure.code, "DSIVIO_RESULT_MISSING");
});

test("doctor names what Hypit can use, what it cannot, and models Dsivio knows nothing about", async () => {
  const offers = offersFrom(models);
  const cliJson = JSON.stringify(models);
  const fake = join(await mkdtemp(join(tmpdir(), "fake-dsivio-")), "dsivio");
  await writeFile(fake, `#!/bin/sh\necho '${cliJson.replaceAll("'", "'\\''")}'\n`, { mode: 0o755 });
  const diagnostics = await probeDsivio({ command: fake }, offers);
  const byCode = (code) => diagnostics.filter((item) => item.code === code);
  assert.equal(byCode("DSIVIO_MEDIA_READY").length, 1);
  assert.match(byCode("DSIVIO_MEDIA_READY")[0].message, /seedance-2\.5 <- p\/doubao-seedance-2-5-260628/u);
  assert.deepEqual(byCode("DSIVIO_MODEL_NOT_IN_HYPIT").map((item) => item.message.split(" ")[0]).sort(), ["p/gemini-3.1-flash-image", "p/my-private-video", "p/veo-3.1-generate-preview"]);
  assert.equal(byCode("DSIVIO_MODEL_UNKNOWN").length, 0, "an unknown model that no row matches is reported as not usable, not as unknown");
  const unknownServed = await probeDsivio({ command: fake }, offersFrom([video("doubao-seedance-2-5-260628", null), image("gpt-image-2.5-flare", GPT_IMAGE)]));
  assert.ok(unknownServed.some((item) => item.code === "DSIVIO_MODEL_UNKNOWN"));
});

test("the command is found on PATH, then in Dsivio's launcher directory", () => {
  assert.equal(resolveCommand("/abs/dsivio"), "/abs/dsivio");
  assert.equal(typeof resolveCommand(undefined), "string");
});

test("fulfilments picks by Dsivio's order and never lets one Dsivio model serve a Hypit model twice", () => {
  const found = fulfilments("video", models);
  const hypitNames = found.map(({ row }) => row.hypit.name);
  assert.equal(new Set(hypitNames).size, hypitNames.length);
  assert.deepEqual(hypitNames.sort(), ["grok-imagine-video-1.5-preview", "minimax-h3", "seedance-2.5"]);
});

test("with nothing to offer the endpoint still loads, refuses every request with the reason, and doctor is not fooled", async () => {
  const provider = createDsivioProvider({ instance: "d", pool: "d", offers: placeholderOffers("open Dsivio and enable models"), run: async () => ({}) });
  assert.ok(provider.capabilities.length > 0, "Hypit rejects an endpoint that declares nothing");
  for (const entry of provider.capabilities) {
    const verdict = entry.supports({ constraints: { ports: { prompt: ["p"] } } });
    assert.equal(verdict.status, "unsupported");
    assert.match(verdict.reason, /open Dsivio and enable models/u);
  }
  const fake = join(await mkdtemp(join(tmpdir(), "fake-dsivio-")), "dsivio");
  await writeFile(fake, `#!/bin/sh\necho '[]'\n`, { mode: 0o755 });
  const diagnostics = await probeDsivio({ command: fake }, placeholderOffers("x"));
  assert.deepEqual(diagnostics.map((item) => item.code), ["DSIVIO_NO_MODEL"]);
});

test("image sizes and last-frame pairing follow the selected Dsivio capabilities", () => {
  const offer = offersFrom([image("gpt-image-2.5-flare", { ...GPT_IMAGE, sizes: ["1K", "2K", "4K"] })])[0];
  assert.equal(checkLimits(offer, { resolution: ["512"] }).status, "unsupported");
  assert.equal(checkLimits(offer, { resolution: ["2K"] }).status, "supported");
  const h3 = offersFrom([video("MiniMax-H3", { ...H3, lastFrameNeedsFirst: false })])[0];
  assert.equal(checkLimits(h3, { lastFrame: [{}] }).status, "supported");
  const seedance = offersFrom([video("doubao-seedance-2-5-260628", SEEDANCE)])[0];
  assert.equal(checkLimits(seedance, { lastFrame: [{}] }).status, "unsupported");
});

test("a receipt Dsivio could not find again is its own failure, keeping the receipt for the provider", async () => {
  const dsivio = scripted([{ id: "original", status: "failed", canResume: true, remoteId: "remote-1", error: "供应商受理后一直查不到该任务（回执 remote-1），可能已扣费" }]);
  const provider = providerFor(models, dsivio.run);
  const { endpoint } = capabilityOf(provider, "grok-imagine-video-1.5-preview");
  const result = await endpoint.poll({ handle: { id: "original" }, command: { id: "verify" } });
  assert.equal(result.status, "failed");
  assert.equal(result.failure.code, "DSIVIO_RECEIPT_LOST");
  assert.match(result.failure.message, /remote-1/u);
  assert.deepEqual(result.receipt, { id: "original" });
  assert.deepEqual(dsivio.calls.map((c) => c.args), [["media", "status", "original"]]);
});

test("polling keeps the paid task pending while Dsivio is closed, and says to open it", async () => {
  const closed = Object.assign(new Error("Dsivio media status exited 6: Dsivio is not running"), { code: "DSIVIO_NOT_RUNNING", exitCode: 6 });
  const dsivio = scripted([closed, { id: "t", status: "running" }]);
  const provider = providerFor(models, dsivio.run);
  const { endpoint } = capabilityOf(provider, "grok-imagine-video-1.5-preview");
  const waiting = await endpoint.poll({ handle: { id: "t" }, command: { id: "c" } });
  assert.equal(waiting.status, "pending");
  assert.deepEqual(waiting.handle, { id: "t" });
  assert.match(waiting.progress.phase, /open Dsivio/u);
  assert.equal((await endpoint.poll({ handle: { id: "t" }, command: { id: "c" } })).status, "pending");
});

test("an auto aspect ratio lets Dsivio use the model default instead of sending an unsupported value", async () => {
  const dsivio = scripted([{ id: "task-auto", status: "running", outputs: [] }]);
  const provider = providerFor(models, dsivio.run);
  const { context } = startContext({ ports: { prompt: ["p"], aspectRatio: ["auto"], resolution: ["720p"], duration: [5] } });
  await capabilityOf(provider, "grok-imagine-video-1.5-preview").endpoint.start(context);
  assert.ok(!dsivio.calls[0].args.includes("--ratio"), dsivio.calls[0].args.join(" "));
  // The image command accepts `auto` itself, so it is passed through there.
  const imageDsivio = scripted([{ id: "img", status: "running", outputs: [] }]);
  const images = providerFor(models, imageDsivio.run);
  const { context: imageContext } = startContext({ ports: { prompt: ["p"], aspectRatio: ["auto"], resolution: ["1K"] } });
  await capabilityOf(images, "gpt-image-2").endpoint.start(imageContext);
  const args = imageDsivio.calls[0].args;
  assert.equal(args[args.indexOf("--ratio") + 1], "auto");
});
