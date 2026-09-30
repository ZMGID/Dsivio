import {
  createRuntimeEndpointAdapterFacet,
  runtimeConfigExact,
  runtimeConfigObject,
  runtimeConfigPositiveInteger,
  runtimeConfigString,
} from "@hypit/hypit/runtime-kit";

import { createDsivioProvider, listModels, offersFrom, placeholderOffers, probeDsivio, providerModule } from "./provider.js";

// No model, service address or key belongs here: Dsivio owns the enabled models (Settings > 媒体创作).
// `imageModel` / `videoModel` are optional pins (`provider/model` from `dsivio media models`).
const CONFIG_KEYS = ["command", "imageModel", "videoModel", "pollIntervalMs", "requestTimeoutMs", "concurrency"];

export function readDsivioConfig(instance, config) {
  const declared = runtimeConfigObject(config ?? {}, "Dsivio media");
  runtimeConfigExact(declared, CONFIG_KEYS, "Dsivio media");
  return {
    instance,
    command: runtimeConfigString(declared.command, "Dsivio media command"),
    imageModel: runtimeConfigString(declared.imageModel, "Dsivio media imageModel"),
    videoModel: runtimeConfigString(declared.videoModel, "Dsivio media videoModel"),
    pollIntervalMs: runtimeConfigPositiveInteger(declared.pollIntervalMs, "Dsivio media pollIntervalMs"),
    requestTimeoutMs: runtimeConfigPositiveInteger(declared.requestTimeoutMs, "Dsivio media requestTimeoutMs"),
    concurrency: runtimeConfigPositiveInteger(declared.concurrency, "Dsivio media concurrency"),
  };
}

export default {
  format: "hypit.node-package@1",
  hostFacets: [createRuntimeEndpointAdapterFacet({
    use: providerModule.name,
    async activate(context) {
      const options = { pool: context.pool ?? context.instance, ...readDsivioConfig(context.instance, context.config) };
      // The models Dsivio serves decide which Hypit capabilities this endpoint offers, so ask once now.
      const listed = await listModels(options);
      const found = offersFrom(listed, options);
      const offers = found.length > 0 ? found : placeholderOffers(
        listed.length === 0
          ? "Dsivio did not answer or has no image/video model enabled: open Dsivio and enable models in Settings > 媒体创作, then restart the Hypit Runtime"
          : "none of the models enabled in Dsivio matches a Hypit model this adapter can serve",
      );
      return { endpoint: createDsivioProvider({ ...options, offers }), diagnose: async () => await probeDsivio(options, offers) };
    },
  })],
};
