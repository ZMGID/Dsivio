export class EndpointServiceError extends Error { constructor(code, message) { super(message); this.code = code; } }
export const canonicalize = (value) => value;
export const wakeAfter = (handle, delayMs, now = 0, progress) => ({ status: "pending", handle, wakeAt: now + delayMs, ...(progress ? { progress } : {}) });
export const defineEndpointPackage = (options) => options;
