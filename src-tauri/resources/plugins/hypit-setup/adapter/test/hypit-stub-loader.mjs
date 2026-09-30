// Test-only: resolve the @hypit/hypit/* imports to tiny local stand-ins.
export async function resolve(specifier, context, next) {
  if (specifier.startsWith("@hypit/hypit/")) return { url: new URL(`./stubs/${specifier.split("/").pop()}.mjs`, import.meta.url).href, shortCircuit: true };
  return next(specifier, context);
}
