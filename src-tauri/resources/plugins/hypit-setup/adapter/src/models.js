// The map between Hypit's models and the models Dsivio serves.
//
// Hypit fixes the request shape of each of its models (its ports). Dsivio decides which concrete
// model answers, from Settings > 媒体创作. A row here says: this Hypit capability is fulfilled by a
// Dsivio model whose id matches `match`, and each Hypit port becomes one `dsivio media` flag.
//
// What a Dsivio model really accepts comes from `dsivio media models` (its `capabilities`), not from
// this file: a port is offered to Hypit only when the model chosen in Dsivio can carry it, so a model
// without last-frame support never receives a last frame. Ports with no row here are refused up
// front rather than dropped.

/** Hypit port -> command line. `kind` tells how the value is carried. */
const PORTS = {
  prompt: { kind: "prompt" },
  aspectRatio: { kind: "flag", flag: "--ratio" },
  resolution: { kind: "flag", flag: "--resolution" },
  duration: { kind: "flag", flag: "--duration" },
  generateAudio: { kind: "switch", flag: "--audio" },
  // Hypit requires `webSearch` on Seedance. Dsivio has no web-search tool, so `false` (the meaning of
  // "not asked for") is accepted and sends nothing, and `true` is refused before submitting.
  webSearch: { kind: "off-only" },
  firstFrame: { kind: "media", flag: "--first-frame", single: true },
  lastFrame: { kind: "media", flag: "--last-frame", single: true },
  referenceImage: { kind: "media", flag: "--ref" },
  referenceVideo: { kind: "media", flag: "--ref-video" },
  referenceAudio: { kind: "media", flag: "--ref-audio" },
};
const IMAGE_PORTS = {
  prompt: { kind: "prompt" },
  aspectRatio: { kind: "flag", flag: "--ratio" },
  resolution: { kind: "flag", flag: "--size" },
  images: { kind: "media", flag: "--ref" },
};

/** Which Dsivio capability field limits which Hypit port (a port with no entry is always offered). */
const VIDEO_LIMITS = {
  firstFrame: (c) => c.firstFrame === true,
  lastFrame: (c) => c.lastFrame === true,
  referenceImage: (c) => c.maxReferenceImages > 0,
  referenceVideo: (c) => c.maxReferenceVideos > 0,
  referenceAudio: (c) => c.maxReferenceAudios > 0,
  generateAudio: (c) => c.audioToggle === true,
  // Hypit calls the reference pictures of xAI/PixVerse-style models simply `images`.
  images: (c) => c.maxReferenceImages > 0,
};
const IMAGE_LIMITS = { images: (c) => c.maxReferenceImages > 0 };

export const VIDEO_MODELS = [
  {
    hypit: { module: "@hypit/seedance", name: "seedance-2.5" },
    match: /seedance-2-5/iu,
    ports: ["prompt", "aspectRatio", "resolution", "duration", "generateAudio", "webSearch", "firstFrame", "lastFrame", "referenceImage", "referenceVideo", "referenceAudio"],
  },
  {
    hypit: { module: "@hypit/seedance", name: "seedance-2-fast" },
    match: /seedance-2-0-fast/iu,
    ports: ["prompt", "aspectRatio", "resolution", "duration", "generateAudio", "webSearch", "firstFrame", "lastFrame", "referenceImage", "referenceVideo", "referenceAudio"],
  },
  {
    hypit: { module: "@hypit/seedance", name: "seedance-2" },
    match: /seedance-2-0-\d/iu,
    ports: ["prompt", "aspectRatio", "resolution", "duration", "generateAudio", "webSearch", "firstFrame", "lastFrame", "referenceImage", "referenceVideo", "referenceAudio"],
  },
  {
    hypit: { module: "@hypit/minimax-h3", name: "minimax-h3" },
    match: /minimax-h3(?!-max)/iu,
    ports: ["prompt", "aspectRatio", "resolution", "duration", "firstFrame", "lastFrame", "referenceImage", "referenceVideo", "referenceAudio"],
  },
  {
    hypit: { module: "@hypit/grok-imagine", name: "grok-imagine-video-1.5-preview" },
    match: /grok-imagine-video-1\.5/iu,
    ports: ["prompt", "aspectRatio", "resolution", "duration", "images"],
  },
  {
    hypit: { module: "@hypit/grok-imagine", name: "grok-imagine-video" },
    match: /^grok-imagine-video$/iu,
    ports: ["prompt", "aspectRatio", "resolution", "duration", "images"],
  },
];

// Only models whose required Hypit ports Dsivio can all carry belong here. Left out on purpose:
//  - seedream-5-lite: requires `quality` (basic/high/ultra), `outputFormat` and `nsfwCheck`, none of which
//    `dsivio media image` accepts (its `--quality` is low/medium/high);
//  - nano-banana-*: requires `outputFormat`.
// `hypit doctor` reports a Dsivio model that lands on none of these rows, so nothing is dropped silently.
export const IMAGE_MODELS = [
  { hypit: { module: "@hypit/gpt-image", name: "gpt-image-2" }, match: /gpt-image/iu, ports: ["prompt", "aspectRatio", "resolution", "images"] },
];

export const PORT_TABLES = { video: PORTS, image: IMAGE_PORTS };
export const PORT_LIMITS = { video: VIDEO_LIMITS, image: IMAGE_LIMITS };

/** Hypit's own port names for reference pictures differ per model; `images` and `referenceImage` are both `--ref`. */
PORT_TABLES.video.images = { kind: "media", flag: "--ref" };

/** The first Dsivio model (in Dsivio's priority order) that a table row fulfils, per Hypit capability. */
export function fulfilments(kind, listed) {
  const table = kind === "image" ? IMAGE_MODELS : VIDEO_MODELS;
  const models = listed.filter((entry) => entry?.kind === kind);
  const found = [];
  for (const row of table) {
    const model = models.find((entry) => row.match.test(entry.model));
    if (model !== undefined) found.push({ row, model });
  }
  return found;
}

/** Ports of `row` that this Dsivio model can carry right now. Unknown models carry the prompt only. */
export function portsFor(kind, row, model) {
  const limits = PORT_LIMITS[kind];
  const capabilities = model.capabilities;
  if (capabilities === null || capabilities === undefined) return row.ports.filter((port) => port === "prompt");
  return row.ports.filter((port) => limits[port] === undefined || limits[port](capabilities));
}
