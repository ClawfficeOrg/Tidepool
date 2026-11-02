import type { ClientOptions, TidepoolClient } from "./client.js";

export * from "./index.js";

/** Read a bearer token from a file and return it trimmed. */
export declare function readTokenFile(path: string): Promise<string>;

/** Build a client whose bearer token comes from `tokenFile`. */
export declare function createNodeClient(
  options: Omit<ClientOptions, "token"> & { tokenFile?: string },
): Promise<TidepoolClient>;
