// Generated from docs/api/openapi-v1.json. Do not edit by hand.
// Regenerate: npm run generate (from sdk/ts).

/** One route's access model and wire shape, from the contract. */
export interface RouteInfo {
  readonly operationId: string;
  readonly method: string;
  readonly path: string;
  readonly namespace: readonly string[];
  readonly name: string;
  readonly pathParams: readonly string[];
  readonly query: Readonly<Record<string, string>> | null;
  readonly body: string | null;
  readonly response: string;
  readonly success: readonly number[];
  readonly errors: readonly number[];
  readonly rateLimited: boolean;
  readonly auth: "public" | "member" | "member_or_api_key" | "api_key" | "host" | "signature";
  readonly scope: string | null;
  readonly role: string | null;
  readonly surface: string;
  readonly deprecated: boolean;
  readonly summary: string;
}

/** Contract version of the committed document. */
export declare const CONTRACT_VERSION: string;

/** Every route in the committed contract, keyed by operation id. */
export declare const ROUTES: Readonly<Record<string, RouteInfo>>;

/** Operation ids in contract order. */
export declare const OPERATIONS: readonly string[];

/** API-key scopes. */
export declare const SCOPES: readonly string[];

/** Member roles. */
export declare const ROLES: readonly string[];

/** Route metadata by operation id, or undefined. */
export declare function route(operationId: string): RouteInfo | undefined;
