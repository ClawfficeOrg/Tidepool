//! Browser entry point.
//!
//! Deliberately a thin re-export: the browser build must reach no Node-only
//! module. `test/browser.test.mjs` walks the import graph from here and fails
//! on any `node:` or bare specifier.

export * from "./index.js";
