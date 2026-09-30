const text = (value) => (value === undefined ? undefined : String(value));
export const createRuntimeEndpointAdapterFacet = (facet) => facet;
export const runtimeConfigObject = (value) => value;
export const runtimeConfigExact = (value, keys, subject) => {
  for (const key of Object.keys(value)) if (!keys.includes(key)) throw new Error(`${subject}: Unknown key ${key}`);
};
export const runtimeConfigString = text;
export const runtimeConfigPositiveInteger = (value) => value;
