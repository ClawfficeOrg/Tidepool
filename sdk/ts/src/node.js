//! Node entry point.
//!
//! Adds only what a browser cannot do: read a bearer token from a file (CI,
//! `~/.config/tidepool/token`) so a script never has to put a credential in
//! its command line. Everything else is re-exported from the browser-safe core.

import { readFile } from "node:fs/promises";

import { TidepoolClient } from "./client.js";

export * from "./index.js";

/**
 * Read a bearer token from a file and return it trimmed.
 *
 * @param {string} path
 * @returns {Promise<string>}
 */
export async function readTokenFile(path) {
  const token = (await readFile(path, "utf8")).trim();
  if (!token) throw new Error(`token file is empty: ${path}`);
  return token;
}

/**
 * Build a client whose token comes from `options.token`, `options.tokenFile`
 * or nothing (public routes only).
 *
 * @param {Omit<ConstructorParameters<typeof TidepoolClient>[0], "token"> & { tokenFile?: string }} options
 */
export async function createNodeClient(options) {
  const { tokenFile, ...rest } = options;
  const token = tokenFile ? await readTokenFile(tokenFile) : undefined;
  return new TidepoolClient({ ...rest, token });
}
