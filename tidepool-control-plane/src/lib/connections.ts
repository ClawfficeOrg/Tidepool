/**
 * Connections: the model behind the connections page.
 *
 * A connection is one provider account the pool may receive events from —
 * GitHub, Slack or Discord. Each provider has its own transport, its own
 * credentials and its own resource identity, and the page has to explain all
 * three without ever reading a credential back.
 *
 * # What this module owns
 *
 * Everything a person reads is decided here, not in a component:
 *
 * - the provider catalogue: GitHub over a webhook, Slack over a webhook or
 *   socket mode, Discord over the bot gateway;
 * - the exact callback URL for the pool's current origin, and the URLs that
 *   must change when that origin moves;
 * - the write-only credential field: it knows only whether a credential is
 *   set, never its value, and offers replacement rather than display;
 * - the verification checks, each with the provider's own words, so a failed
 *   verification names the check that failed.
 *
 * # Two honest gaps in this contract revision
 *
 * `ConnectionView` does not carry the transport a Slack connection uses, and
 * `PATCH /v1/connections/{id}` cannot replace a credential. The page therefore
 * remembers the transport chosen here per connection id (a browser preference,
 * not pool state) and defaults to the webhook otherwise, and the replace flow
 * reports that the contract has no rotation operation rather than pretending a
 * write succeeded. Both gaps are stated on the page; neither is approximated
 * with a secret read.
 */

import type {
  ConnectionView as ContractConnectionView,
  ConnectionsResponse,
  CreateConnectionRequest,
  ResourceRequest,
  TidepoolClient,
  UpdateConnectionRequest,
} from '@clawffice/tidepool-sdk';
import { normalisePoolUrl } from './api/pool';
import { presentProblem, type PoolProblem } from './api/problem';
import type { StatusKind } from './status';

/** The provider kinds the hub contract knows. */
export type ConnectionProvider = 'github' | 'slack' | 'discord';

/** Every provider kind, in the order the page lists them. */
export const CONNECTION_PROVIDERS: readonly ConnectionProvider[] = ['github', 'slack', 'discord'];

/** The transports the three providers expose. */
export type ConnectionTransport =
  | 'github_webhook'
  | 'slack_webhook'
  | 'slack_socket'
  | 'discord_gateway';

/** One credential field a provider needs inside its secret JSON object. */
export interface CredentialField {
  /** The key inside the secret-store JSON object. */
  key: string;
  /** What the provider calls it. */
  label: string;
  /** One line on where to find it. */
  description: string;
}

/** One provider transport, in every form the page renders it. */
export interface TransportDescriptor {
  /** Stable id. */
  id: ConnectionTransport;
  /** Provider that owns it. */
  provider: ConnectionProvider;
  /** The word shown in the transport choice. */
  label: string;
  /** One line on how events arrive. */
  description: string;
  /** True when the provider must be able to reach the pool. */
  needsPublicAddress: boolean;
  /** Callback path template, or null when the pool dials out instead. */
  callbackTemplate: string | null;
  /** Credential keys this transport needs. */
  credentialKeys: readonly string[];
  /** What the page says when no callback URL exists. */
  noCallbackNote: string;
}

/** One provider, in every form the page renders it. */
export interface ProviderDescriptor {
  /** Provider kind. */
  provider: ConnectionProvider;
  /** The provider's own name. */
  label: string;
  /** One line on what the connection receives. */
  summary: string;
  /** Resource kinds the provider accepts. */
  resourceKinds: readonly string[];
  /** Credential fields the provider needs. */
  credentialFields: readonly CredentialField[];
  /** Transports this provider exposes, in preference order. */
  transports: readonly TransportDescriptor[];
}

const GITHUB_WEBHOOK: TransportDescriptor = {
  id: 'github_webhook',
  provider: 'github',
  label: 'Webhook',
  description:
    'GitHub posts each delivery to the pool. Configure the App webhook at the callback URL below.',
  needsPublicAddress: true,
  callbackTemplate: '/v1/webhooks/github/{slug}',
  credentialKeys: ['app_id', 'private_key', 'webhook_secret'],
  noCallbackNote: 'The pool has no public address, so the GitHub callback URL cannot be built.',
};

const SLACK_WEBHOOK: TransportDescriptor = {
  id: 'slack_webhook',
  provider: 'slack',
  label: 'Webhook',
  description:
    'Slack posts each event to the pool. Configure Event Subscriptions at the callback URL below.',
  needsPublicAddress: true,
  callbackTemplate: '/v1/webhooks/slack/{slug}',
  credentialKeys: ['bot_token', 'signing_secret'],
  noCallbackNote:
    'The pool has no public address, so the Slack webhook callback URL cannot be built. Use socket mode instead.',
};

const SLACK_SOCKET: TransportDescriptor = {
  id: 'slack_socket',
  provider: 'slack',
  label: 'Socket mode',
  description:
    'The pool dials Slack with the app token, so this pool needs no public address and no callback URL.',
  needsPublicAddress: false,
  callbackTemplate: null,
  credentialKeys: ['bot_token', 'signing_secret', 'app_token'],
  noCallbackNote: 'No callback URL: socket mode dials Slack with app_token; no public address is needed.',
};

const DISCORD_GATEWAY: TransportDescriptor = {
  id: 'discord_gateway',
  provider: 'discord',
  label: 'Bot gateway',
  description:
    'Discord has no inbound webhook for bots. The pool dials the gateway with bot_token and identifies for the guilds this connection owns.',
  needsPublicAddress: false,
  callbackTemplate: null,
  credentialKeys: ['bot_token'],
  noCallbackNote: 'No callback URL: the pool dials Discord’s bot gateway with bot_token.',
};

/**
 * The provider catalogue.
 *
 * The credential keys are the ones the pool's provider implementations read;
 * the callback paths are the routes the pool actually mounts. A new provider
 * adds one entry here, which is what makes the create form and the callback
 * block follow without another component change.
 */
export const CONNECTION_PROVIDERS_CATALOG: Readonly<
  Record<ConnectionProvider, ProviderDescriptor>
> = {
  github: {
    provider: 'github',
    label: 'GitHub',
    summary: 'A GitHub App: repository events arrive by webhook with a signature.',
    resourceKinds: ['repository'],
    credentialFields: [
      { key: 'app_id', label: 'App id', description: 'The GitHub App id.' },
      { key: 'private_key', label: 'Private key', description: 'The App private key PEM.' },
      {
        key: 'webhook_secret',
        label: 'Webhook secret',
        description: 'The secret configured on the App webhook.',
      },
    ],
    transports: [GITHUB_WEBHOOK],
  },
  slack: {
    provider: 'slack',
    label: 'Slack',
    summary: 'A Slack app: mentions arrive by webhook or socket mode.',
    resourceKinds: ['workspace', 'channel'],
    credentialFields: [
      { key: 'bot_token', label: 'Bot token', description: 'The xoxb-… bot token.' },
      {
        key: 'signing_secret',
        label: 'Signing secret',
        description: 'The signing secret from the Slack app.',
      },
      {
        key: 'app_token',
        label: 'App token',
        description: 'The xapp-… token, needed for socket mode only.',
      },
    ],
    transports: [SLACK_WEBHOOK, SLACK_SOCKET],
  },
  discord: {
    provider: 'discord',
    label: 'Discord',
    summary: 'A Discord bot: mentions arrive over the gateway.',
    resourceKinds: ['guild', 'channel'],
    credentialFields: [
      { key: 'bot_token', label: 'Bot token', description: 'The Discord bot token.' },
    ],
    transports: [DISCORD_GATEWAY],
  },
};

/** True when `value` names a provider this revision knows. */
export function isConnectionProvider(value: unknown): value is ConnectionProvider {
  return typeof value === 'string' && (CONNECTION_PROVIDERS as readonly string[]).includes(value);
}

/** Look a provider up, returning null for an unknown one. */
export function describeProvider(value: string | null | undefined): ProviderDescriptor | null {
  return isConnectionProvider(value) ? CONNECTION_PROVIDERS_CATALOG[value] : null;
}

/** Every transport in the catalogue, flattened. */
export const CONNECTION_TRANSPORTS: readonly TransportDescriptor[] = CONNECTION_PROVIDERS.flatMap(
  (provider) => CONNECTION_PROVIDERS_CATALOG[provider].transports,
);

/** True when `value` names a transport this revision knows. */
export function isConnectionTransport(value: unknown): value is ConnectionTransport {
  return typeof value === 'string' && CONNECTION_TRANSPORTS.some((item) => item.id === value);
}

/** Look a transport up, returning null for an unknown one. */
export function describeTransport(
  value: string | null | undefined,
): TransportDescriptor | null {
  return CONNECTION_TRANSPORTS.find((item) => item.id === value) ?? null;
}

/** The transports a provider offers. */
export function transportsFor(provider: ConnectionProvider): readonly TransportDescriptor[] {
  return CONNECTION_PROVIDERS_CATALOG[provider].transports;
}

/** The transport a connection uses when the page has no remembered choice. */
export function defaultTransport(provider: ConnectionProvider): ConnectionTransport {
  return CONNECTION_PROVIDERS_CATALOG[provider].transports[0]?.id ?? 'github_webhook';
}

/** True when a transport belongs to a provider. */
export function transportBelongsTo(
  provider: ConnectionProvider,
  transport: ConnectionTransport,
): boolean {
  return transportsFor(provider).some((item) => item.id === transport);
}

/** The longest slug the pool keeps (matching `MAX_SLUG_LEN`). */
export const MAX_SLUG_LENGTH = 63;

/**
 * Normalise a slug exactly as the pool does: ASCII letters and digits survive,
 * everything else collapses to a hyphen, the result is lowercase and capped.
 */
export function normaliseSlug(raw: string): string {
  let slug = '';
  let pendingHyphen = false;
  for (const character of raw) {
    if (/[A-Za-z0-9]/.test(character)) {
      if (pendingHyphen && slug !== '') slug += '-';
      pendingHyphen = false;
      slug += character.toLowerCase();
    } else {
      pendingHyphen = true;
    }
  }
  const trimmed = slug.replace(/^-+|-+$/g, '');
  return trimmed.slice(0, MAX_SLUG_LENGTH).replace(/-+$/, '');
}

/** The pool HTTP origin a callback URL is built from, or null. */
export function poolOrigin(poolUrl: string | null | undefined): string | null {
  const normalised = normalisePoolUrl(poolUrl);
  if (!normalised) return null;
  try {
    return new URL(normalised).origin;
  } catch {
    return null;
  }
}

/** The callback path for a transport and slug, or null when it dials out. */
export function callbackPath(
  transport: ConnectionTransport,
  slug: string,
): string | null {
  const descriptor = describeTransport(transport);
  if (!descriptor || descriptor.callbackTemplate === null) return null;
  return descriptor.callbackTemplate.replace('{slug}', encodeURIComponent(slug));
}

/**
 * The exact callback URL for a provider transport on the pool's current origin.
 *
 * @returns null for a transport the pool dials out (socket mode, gateway) or
 *   when the pool URL is not an absolute http(s) URL
 */
export function callbackUrl(input: {
  poolUrl: string | null | undefined;
  transport: ConnectionTransport;
  slug: string;
}): string | null {
  const origin = poolOrigin(input.poolUrl);
  const path = callbackPath(input.transport, input.slug);
  if (!origin || !path) return null;
  return `${origin}${path}`;
}

/** What the page shows for a connection's event ingress. */
export interface CallbackView {
  /** `webhook` when a URL exists, `outbound` when the pool dials out. */
  kind: 'webhook' | 'outbound' | 'unavailable';
  /** The exact URL, when there is one. */
  url: string | null;
  /** One line explaining what to do with it, or why there is none. */
  note: string;
  /** The transport this view describes. */
  transport: ConnectionTransport;
}

/**
 * Build the callback block for one connection.
 *
 * A webhook with no public origin is `unavailable`, not `outbound`: the
 * difference matters, because the operator has to fix the pool URL or choose
 * socket mode.
 */
export function callbackView(input: {
  poolUrl: string | null | undefined;
  transport: ConnectionTransport;
  slug: string;
}): CallbackView {
  const descriptor = describeTransport(input.transport);
  if (!descriptor) {
    return {
      kind: 'unavailable',
      url: null,
      note: 'This control plane does not know that transport.',
      transport: input.transport,
    };
  }
  if (descriptor.callbackTemplate === null) {
    return {
      kind: 'outbound',
      url: null,
      note: descriptor.noCallbackNote,
      transport: input.transport,
    };
  }
  const url = callbackUrl(input);
  if (!url) {
    return {
      kind: 'unavailable',
      url: null,
      note: descriptor.noCallbackNote,
      transport: input.transport,
    };
  }
  const provider = CONNECTION_PROVIDERS_CATALOG[descriptor.provider].label;
  return {
    kind: 'webhook',
    url,
    note: `Point the ${provider} webhook at this exact URL.`,
    transport: input.transport,
  };
}

/** The credential field for a connection, in every form the page renders it. */
export interface SecretFieldView {
  /** True when the pool reports a credential reference that resolves. */
  configured: boolean;
  /** The word shown beside the field. Never empty. */
  word: string;
  /** One line explaining the write-only rule. */
  description: string;
  /** The control offered: replacement only. Never "show". */
  actionLabel: string;
  /**
   * Always null. The view model has no slot for a secret, so no code path can
   * put one in it — which is the testable half of the write-only rule.
   */
  value: null;
}

/**
 * Build the write-only credential field.
 *
 * `credential_configured` is a boolean in the contract; the value it refers to
 * is resolved in the pool and never projected. This function therefore has no
 * input that could carry a secret, and returns no output that could show one.
 */
export function secretFieldView(configured: boolean): SecretFieldView {
  return {
    configured,
    word: configured ? 'Credential set' : 'No credential set',
    description: configured
      ? 'The pool holds a credential for this connection and never sends it back. Enter a new one only to replace it.'
      : 'No credential is configured. The pool will not accept events until one is set.',
    actionLabel: configured ? 'Replace credential' : 'Set credential',
    value: null,
  };
}

/** Which connection status the pool reported. */
export type ConnectionStatusValue = 'active' | 'disabled' | null;

/** True when `value` is one of the two contract statuses. */
export function isConnectionStatus(value: unknown): value is Exclude<ConnectionStatusValue, null> {
  return value === 'active' || value === 'disabled';
}

/** One connection, as the page renders it. */
export interface ConnectionRow {
  /** Public id, or `env:<slug>` for environment-managed entries. */
  id: string;
  /** Friendly handle, also the callback path segment. */
  slug: string;
  /** Name a person sees. */
  displayName: string;
  /** Provider descriptor, or null for a kind this revision does not know. */
  provider: ProviderDescriptor | null;
  /** Provider kind as the pool spelled it. */
  providerId: string;
  /** The transport the page believes this connection uses. */
  transport: ConnectionTransport;
  /** Active or disabled, with a word for each. */
  status: { value: ConnectionStatusValue; word: string; tone: StatusKind };
  /** Database or environment managed. */
  managedBy: 'database' | 'environment' | 'unknown';
  /** True only for a database-managed connection; environment entries are read-only. */
  editable: boolean;
  /** The write-only credential field. */
  credential: SecretFieldView;
  /** The exact callback URL, or why there is none. */
  callback: CallbackView;
  /** Provider resources this connection delivers for. */
  resources: readonly ResourceView[];
  /** When the connection was created. */
  createdAt: string | null;
}

/** One resource a connection addresses. */
export interface ResourceView {
  /** `repository`, `workspace`, `guild`, `channel`, ... */
  kind: string;
  /** Provider-side id. */
  externalId: string;
  /** Optional human label. */
  displayName: string | null;
}

/** The connection shape this module reads; a superset of the contract view. */
export interface ConnectionLike {
  id: string;
  provider: string;
  slug: string;
  display_name: string;
  status: string;
  managed_by: string;
  credential_configured: boolean;
  resources: readonly {
    resource_kind: string;
    external_id: string;
    display_name?: string | null;
  }[];
  created_at?: string | null;
}

/** Describe the status the pool reported. */
export function connectionStatus(value: string | null | undefined): {
  value: ConnectionStatusValue;
  word: string;
  tone: StatusKind;
} {
  if (value === 'active') return { value: 'active', word: 'Active', tone: 'success' };
  if (value === 'disabled') return { value: 'disabled', word: 'Disabled', tone: 'neutral' };
  return { value: null, word: 'Unknown', tone: 'unknown' };
}

/** The managed-by label a person reads. */
export function managedByLabel(value: string): 'database' | 'environment' | 'unknown' {
  if (value === 'database') return 'database';
  if (value === 'environment') return 'environment';
  return 'unknown';
}

/**
 * Build one row from a pool response.
 *
 * @param connection the contract view, or anything shaped like it
 * @param options transport override and the pool URL used for the callback
 */
export function connectionRow(
  connection: ConnectionLike,
  options: {
    poolUrl?: string | null;
    transport?: ConnectionTransport | null;
  } = {},
): ConnectionRow {
  const provider = describeProvider(connection.provider);
  const transport =
    provider &&
    options.transport &&
    isConnectionTransport(options.transport) &&
    transportBelongsTo(provider.provider, options.transport)
      ? options.transport
      : provider
        ? defaultTransport(provider.provider)
        : 'github_webhook';
  const managedBy = managedByLabel(connection.managed_by);
  const status = connectionStatus(connection.status);
  return {
    id: connection.id,
    slug: connection.slug,
    displayName: connection.display_name,
    provider,
    providerId: connection.provider,
    transport,
    status,
    managedBy,
    editable: managedBy === 'database',
    credential: secretFieldView(connection.credential_configured),
    callback: callbackView({
      poolUrl: options.poolUrl,
      transport,
      slug: connection.slug,
    }),
    resources: connection.resources.map((resource) => ({
      kind: resource.resource_kind,
      externalId: resource.external_id,
      displayName: resource.display_name ?? null,
    })),
    createdAt: connection.created_at ?? null,
  };
}

/** Order rows by provider then slug, so the list is stable between reloads. */
export function sortConnectionRows(rows: readonly ConnectionRow[]): ConnectionRow[] {
  const rank = (row: ConnectionRow): number => {
    switch (row.providerId) {
      case 'github':
        return 0;
      case 'slack':
        return 1;
      case 'discord':
        return 2;
      default:
        return 3;
    }
  };
  return [...rows].sort((left, right) => {
    const byProvider = rank(left) - rank(right);
    if (byProvider !== 0) return byProvider;
    return left.slug.localeCompare(right.slug);
  });
}

/** One resource in a create form. */
export interface ResourceInput {
  /** Resource kind. */
  kind: string;
  /** Provider-side id. */
  externalId: string;
  /** Optional human label. */
  displayName?: string | null;
}

/** A validated create form, or the reason it is not valid. */
export type ConnectionInputValidation =
  | {
      ok: true;
      input: {
        provider: ConnectionProvider;
        transport: ConnectionTransport;
        slug: string;
        displayName: string;
        credentialRef: string;
        resources: ResourceRequest[];
      };
    }
  | { ok: false; field: string; message: string };

/** What the create form submits. */
export interface NewConnectionInput {
  provider: string;
  transport: string;
  slug: string;
  displayName: string;
  credentialRef: string;
  resources: readonly ResourceInput[];
}

/**
 * Validate the create form before any request is made.
 *
 * The pool remains the authority — a duplicate slug comes back as a conflict —
 * but the messages mirror the pool's own (slug, display name, credential
 * reference, provider resource kind), so the same mistake reads the same way
 * whether it is caught here or there.
 */
export function validateConnectionInput(input: NewConnectionInput): ConnectionInputValidation {
  if (!isConnectionProvider(input.provider)) {
    return { ok: false, field: 'provider', message: 'Choose GitHub, Slack or Discord.' };
  }
  const provider = CONNECTION_PROVIDERS_CATALOG[input.provider];
  if (!isConnectionTransport(input.transport) || !transportBelongsTo(input.provider, input.transport)) {
    return {
      ok: false,
      field: 'transport',
      message: `${provider.label} does not support that transport.`,
    };
  }
  const slug = normaliseSlug(input.slug);
  if (slug === '') {
    return { ok: false, field: 'slug', message: 'slug is required' };
  }
  const displayName = input.displayName.trim();
  if (displayName === '' || [...displayName].length > 128) {
    return {
      ok: false,
      field: 'display_name',
      message: 'display_name must be 1-128 characters',
    };
  }
  const credentialRef = input.credentialRef.trim();
  if (credentialRef === '' || credentialRef.length > 255) {
    return {
      ok: false,
      field: 'credential_ref',
      message: 'credential_ref must be 1-255 characters',
    };
  }
  const transport = describeTransport(input.transport);
  if (!transport) {
    return { ok: false, field: 'transport', message: 'This transport is not known.' };
  }

  const resources: ResourceRequest[] = [];
  const seen = new Set<string>();
  for (const resource of input.resources) {
    const kind = resource.kind.trim();
    const externalId = resource.externalId.trim();
    if (kind === '' || externalId === '') continue;
    if (!provider.resourceKinds.includes(kind)) {
      return {
        ok: false,
        field: 'resources',
        message: `${input.provider} resource_kind must be ${provider.resourceKinds
          .map((value) => `'${value}'`)
          .join(' or ')}, got '${kind}'`,
      };
    }
    const key = `${kind}\u0000${externalId.toLowerCase()}`;
    if (seen.has(key)) {
      return { ok: false, field: 'resources', message: 'duplicate resource identity' };
    }
    seen.add(key);
    const display = resource.displayName?.trim() ?? '';
    resources.push({
      resource_kind: kind,
      external_id: externalId,
      ...(display === '' ? {} : { display_name: display }),
    });
  }

  return {
    ok: true,
    input: {
      provider: input.provider,
      transport: input.transport,
      slug,
      displayName,
      credentialRef,
      resources,
    },
  };
}

/** One verification check and its outcome. */
export interface VerificationCheck {
  /** Stable id, used in markup and tests. */
  id:
    | 'pool_answered'
    | 'status_active'
    | 'credential_configured'
    | 'resource_scope'
    | 'transport_ready';
  /** The check's name, shown even when it passes. */
  label: string;
  /** True when this check passed. */
  ok: boolean;
  /** The provider's own words, or a plain explanation of the result. */
  detail: string;
}

/** The result of verifying one connection. */
export interface VerificationReport {
  /** The connection that was verified. */
  connectionId: string;
  /** Provider kind, as the pool spelled it. */
  provider: string;
  /** The transport the checks were run against. */
  transport: ConnectionTransport;
  /** True only when every check passed. */
  ok: boolean;
  /** Every check, in the order they ran. */
  checks: readonly VerificationCheck[];
  /** The checks that failed, in order. */
  failed: readonly VerificationCheck[];
  /** One line naming the failing check, or the all-pass sentence. */
  summary: string;
}

/** The credential sentence a provider uses when its keys are missing. */
export function credentialRequirement(
  provider: ConnectionProvider,
  transport: ConnectionTransport,
): string {
  const descriptor = describeProvider(provider);
  const keys =
    describeTransport(transport)?.credentialKeys ?? descriptor?.credentialFields.map((f) => f.key) ?? [];
  const label = descriptor?.label ?? 'Provider';
  return `${label} credentials need ${keys.join(', ')}.`;
}

/** The resource sentence a provider uses when it has no resource identity. */
export function resourceRequirement(provider: ConnectionProvider): string {
  const descriptor = describeProvider(provider);
  const kinds = descriptor?.resourceKinds.filter((kind) => kind !== 'channel') ?? [];
  const label = descriptor?.label ?? 'The provider';
  return `No ${kinds.map((kind) => `'${kind}'`).join(' or ')} resource is registered, so no ${label} delivery can match.`;
}

/**
 * Build the verification checks for one connection.
 *
 * Nothing here reads a credential. Each check answers a question the operator
 * can act on, and the failing one is named in the report summary. The pool's
 * own problem detail is used verbatim when a check fails because of it.
 */
export function verificationChecks(
  row: ConnectionRow,
  options: { publicOrigin?: string | null } = {},
): VerificationCheck[] {
  const provider =
    row.provider?.provider ??
    (isConnectionProvider(row.providerId) ? row.providerId : null);
  const transport = describeTransport(row.transport);
  const checks: VerificationCheck[] = [
    {
      id: 'pool_answered',
      label: 'The pool answered',
      ok: true,
      detail: 'The pool returned this connection; the credential reference was not sent.',
    },
    {
      id: 'status_active',
      label: 'The connection is active',
      ok: row.status.value === 'active',
      detail:
        row.status.value === 'active'
          ? 'The pool delivers events for an active connection.'
          : 'The connection is disabled, so the pool does not deliver events for it.',
    },
    {
      id: 'credential_configured',
      label: 'The credential is configured',
      ok: row.credential.configured,
      detail: row.credential.configured
        ? 'A credential is configured; the pool never sends its value back.'
        : provider
          ? credentialRequirement(provider, row.transport)
          : 'The pool reported a provider this control plane does not know, so its credential fields cannot be checked.',
    },
    {
      id: 'resource_scope',
      label: 'The resource scope is registered',
      ok:
        provider !== null &&
        row.resources.some((resource) => {
          const descriptor = describeProvider(provider);
          return descriptor?.resourceKinds.includes(resource.kind) ?? false;
        }),
      detail: (() => {
        if (provider === null) {
          return 'The pool reported a provider this control plane does not know, so its resources cannot be checked.';
        }
        const matching = row.resources.filter((resource) =>
          describeProvider(provider)?.resourceKinds.includes(resource.kind),
        );
        if (matching.length > 0) {
          return `${matching.length} resource${matching.length === 1 ? '' : 's'} registered for this connection.`;
        }
        return resourceRequirement(provider);
      })(),
    },
    {
      id: 'transport_ready',
      label: 'The transport is ready',
      ok: true,
      detail: '',
    },
  ];

  const transportCheck = checks[checks.length - 1];
  if (transportCheck) {
    if (!transport) {
      transportCheck.ok = false;
      transportCheck.detail = 'The page does not know that transport, so it cannot be verified.';
    } else if (transport.callbackTemplate === null) {
      transportCheck.ok = true;
      transportCheck.detail = transport.noCallbackNote;
    } else if (!options.publicOrigin) {
      transportCheck.ok = false;
      transportCheck.detail = transport.noCallbackNote;
    } else {
      transportCheck.ok = true;
      transportCheck.detail = `The callback URL can be built: ${options.publicOrigin}${transport.callbackTemplate.replace(
        '{slug}',
        encodeURIComponent(row.slug),
      )}`;
    }
  }

  return checks;
}

/** Build a report from a row and the checks that ran. */
export function verificationReport(
  row: ConnectionRow,
  checks: readonly VerificationCheck[],
): VerificationReport {
  const failed = checks.filter((check) => !check.ok);
  const first = failed[0];
  return {
    connectionId: row.id,
    provider: row.providerId,
    transport: row.transport,
    ok: failed.length === 0,
    checks,
    failed,
    summary: first
      ? `Verification failed: ${first.label} — ${first.detail}`
      : `Verification passed: all ${checks.length} checks succeeded.`,
  };
}

/** Verify a connection from its row and the pool's public origin. */
export function verifyConnection(
  row: ConnectionRow,
  options: { publicOrigin?: string | null } = {},
): VerificationReport {
  return verificationReport(row, verificationChecks(row, options));
}

/**
 * Build a failed report when the pool could not answer at all.
 *
 * The problem's message is the pool's own sentence, so a failed verification
 * still names the check (`The pool answered`) and quotes the provider.
 */
export function failedVerification(
  input: {
    connectionId: string;
    provider: string;
    transport: ConnectionTransport;
  },
  problem: PoolProblem,
): VerificationReport {
  const check: VerificationCheck = {
    id: 'pool_answered',
    label: 'The pool answered',
    ok: false,
    detail: problem.message,
  };
  return {
    connectionId: input.connectionId,
    provider: input.provider,
    transport: input.transport,
    ok: false,
    checks: [check],
    failed: [check],
    summary: `Verification failed: ${check.label} — ${problem.message}`,
  };
}

/** A callback URL that changes when the pool's public origin changes. */
export interface CallbackChange {
  /** Public connection id. */
  connectionId: string;
  /** Friendly handle. */
  slug: string;
  /** Name a person sees. */
  displayName: string;
  /** Provider label. */
  provider: string;
  /** The transport whose URL changed. */
  transport: ConnectionTransport;
  /** The URL the provider was given before. */
  previousUrl: string;
  /** The URL the provider must be given now. */
  currentUrl: string;
}

/**
 * Compare the callback URLs of every webhook connection across an origin move.
 *
 * A connection that dials out (Slack socket mode, Discord gateway) has no
 * callback URL and is correctly absent: there is nothing at the provider to
 * change. The list is what the page shows, old URL beside new, so the operator
 * can paste both sides.
 */
export function callbackChanges(
  previousPoolUrl: string | null | undefined,
  currentPoolUrl: string | null | undefined,
  rows: readonly ConnectionRow[],
): CallbackChange[] {
  const previousOrigin = poolOrigin(previousPoolUrl);
  const currentOrigin = poolOrigin(currentPoolUrl);
  if (!previousOrigin || !currentOrigin || previousOrigin === currentOrigin) return [];
  const changes: CallbackChange[] = [];
  for (const row of rows) {
    const path = callbackPath(row.transport, row.slug);
    if (!path) continue;
    const previousUrl = `${previousOrigin}${path}`;
    const currentUrl = `${currentOrigin}${path}`;
    if (previousUrl === currentUrl) continue;
    changes.push({
      connectionId: row.id,
      slug: row.slug,
      displayName: row.displayName,
      provider: row.provider?.label ?? row.providerId,
      transport: row.transport,
      previousUrl,
      currentUrl,
    });
  }
  return changes;
}

/** The sentence above the origin-change list. */
export function callbackChangeSummary(
  previousPoolUrl: string | null | undefined,
  currentPoolUrl: string | null | undefined,
): string {
  const previous = poolOrigin(previousPoolUrl) ?? 'no previous origin';
  const current = poolOrigin(currentPoolUrl) ?? 'no current origin';
  return `The pool's public origin changed from ${previous} to ${current}. Update these callback URLs at the provider:`;
}

/** The key the page uses to remember a connection's transport. */
export const TRANSPORT_PREFERENCE_STORAGE_KEY =
  'tidepool.control-plane.connection-transports';

/** The key the page uses to remember the pool origin it last showed. */
export const POOL_ORIGIN_STORAGE_KEY = 'tidepool.control-plane.callback-origin';

/** The storage subset this module needs. */
export interface ConnectionStorage {
  /** Read a saved value. */
  getItem(key: string): string | null;
  /** Write a value, when the store allows it. */
  setItem?(key: string, value: string): void;
}

/** Read the remembered transport for each connection id. */
export function readTransportPreferences(
  storage: ConnectionStorage | null | undefined,
): Record<string, ConnectionTransport> {
  if (!storage) return {};
  let raw: string | null = null;
  try {
    raw = storage.getItem(TRANSPORT_PREFERENCE_STORAGE_KEY);
  } catch {
    return {};
  }
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    const preferences: Record<string, ConnectionTransport> = {};
    for (const [id, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (isConnectionTransport(value)) preferences[id] = value;
    }
    return preferences;
  } catch {
    return {};
  }
}

/** Remember one connection's transport, returning the new preference map. */
export function writeTransportPreference(
  storage: ConnectionStorage | null | undefined,
  connectionId: string,
  transport: ConnectionTransport,
): Record<string, ConnectionTransport> {
  const preferences = readTransportPreferences(storage);
  preferences[connectionId] = transport;
  if (storage?.setItem) {
    try {
      storage.setItem(TRANSPORT_PREFERENCE_STORAGE_KEY, JSON.stringify(preferences));
    } catch {
      // A full or blocked store must not break the page; the choice still
      // applies to this view, it simply will not survive a reload.
    }
  }
  return preferences;
}

/** Remember the origin the page is showing, so a later move is visible. */
export function rememberPoolOrigin(
  storage: ConnectionStorage | null | undefined,
  poolUrl: string | null | undefined,
): void {
  const origin = poolOrigin(poolUrl);
  if (!origin || !storage?.setItem) return;
  try {
    storage.setItem(POOL_ORIGIN_STORAGE_KEY, origin);
  } catch {
    // Same rule as the transport preference: the page keeps working.
  }
}

/** The origin the page last showed, when it differs from the current one. */
export function previousPoolOrigin(
  storage: ConnectionStorage | null | undefined,
  poolUrl: string | null | undefined,
): string | null {
  const current = poolOrigin(poolUrl);
  if (!storage || !current) return null;
  let stored: string | null = null;
  try {
    stored = storage.getItem(POOL_ORIGIN_STORAGE_KEY);
  } catch {
    return null;
  }
  if (!stored || !normalisePoolUrl(stored)) return null;
  return stored === current ? null : stored;
}

/** The pool calls the connections page makes, injectable for tests. */
export interface ConnectionsTransport {
  /** List effective connections. */
  list(): Promise<ConnectionsResponse>;
  /** Create a connection from a credential reference. */
  create(input: CreateConnectionRequest): Promise<ContractConnectionView>;
  /** Read one connection; this is also what verification reads. */
  get(id: string): Promise<ContractConnectionView>;
  /** Rename, enable or disable a database connection. */
  update(id: string, input: UpdateConnectionRequest): Promise<ContractConnectionView>;
  /** Delete a database connection. */
  remove(id: string): Promise<void>;
  /**
   * Replace a credential, when the pool offers the operation. This contract
   * revision does not, so the SDK transport leaves it undefined rather than
   * inventing a request the pool would reject.
   */
  replaceCredential?(id: string, credentialRef: string): Promise<ContractConnectionView>;
}

/** Build the transport from the typed SDK client. */
export function connectionsTransport(client: TidepoolClient): ConnectionsTransport {
  return {
    list: () => client.api.connections.list(),
    create: (input) => client.api.connections.create(input),
    get: (id) => client.api.connections.get(id),
    update: (id, input) => client.api.connections.update(id, input),
    remove: (id) => client.api.connections.remove(id),
  };
}

/** What the page is doing. */
export type ConnectionsPhase = 'idle' | 'loading' | 'ready' | 'error';

/** Everything the connections page renders. */
export interface ConnectionsViewState {
  /** The current load state. */
  phase: ConnectionsPhase;
  /** Connections, in provider then slug order. */
  connections: ConnectionRow[];
  /** The last failure, readable. */
  problem: PoolProblem | null;
  /** The last verification report, until dismissed or replaced. */
  verification: VerificationReport | null;
  /** The connection currently being verified. */
  verifyingId: string | null;
  /** The connection whose action is in flight. */
  busyConnectionId: string | null;
  /** True while a create or action is in flight. */
  busy: boolean;
}

/** The state before anything has been loaded. */
export function emptyConnectionsState(): ConnectionsViewState {
  return {
    phase: 'idle',
    connections: [],
    problem: null,
    verification: null,
    verifyingId: null,
    busyConnectionId: null,
    busy: false,
  };
}

/** How the controller reaches the pool, and what it remembers. */
export interface ConnectionsControllerOptions {
  /** The pool calls. */
  transport: ConnectionsTransport;
  /** Absolute pool URL, used to build callback URLs. */
  poolUrl: string;
  /** Where transport preferences and the last origin live. Omit in tests. */
  storage?: ConnectionStorage | null;
}

/** A problem the control plane raised itself, in the pool's shape. */
function appProblem(input: {
  title: string;
  message: string;
  code: string;
}): PoolProblem {
  return {
    kind: 'app',
    title: input.title,
    message: input.message,
    requestId: null,
    status: null,
    code: input.code,
    type: null,
    retryAfterMs: null,
  };
}

/**
 * The connections page's controller.
 *
 * Every method returns the new state rather than mutating what the caller
 * holds, so a Svelte page can assign the result and a test can assert on it.
 * No method stores a credential value; the only credential fact in state is
 * the contract's `credential_configured` boolean.
 */
export class ConnectionsController {
  private readonly transport: ConnectionsTransport;
  private readonly poolUrl: string;
  private readonly storage: ConnectionStorage | null;
  private preferences: Record<string, ConnectionTransport>;
  private state: ConnectionsViewState = emptyConnectionsState();

  constructor(options: ConnectionsControllerOptions) {
    this.transport = options.transport;
    this.poolUrl = options.poolUrl;
    this.storage = options.storage ?? null;
    this.preferences = readTransportPreferences(this.storage);
  }

  /** The current state, without touching the pool. */
  snapshot(): ConnectionsViewState {
    return this.state;
  }

  /** The selection the page would show for one connection. */
  transportFor(row: { id: string; provider: ProviderDescriptor | null }): ConnectionTransport {
    const remembered = this.preferences[row.id];
    if (remembered && row.provider && transportBelongsTo(row.provider.provider, remembered)) {
      return remembered;
    }
    return row.provider ? defaultTransport(row.provider.provider) : 'github_webhook';
  }

  /** The pool origin callback URLs are built from. */
  get origin(): string | null {
    return poolOrigin(this.poolUrl);
  }

  /** Build a row with this controller's transports and origin. */
  private row(connection: ConnectionLike): ConnectionRow {
    return connectionRow(connection, {
      poolUrl: this.poolUrl,
      transport: this.preferences[connection.id] ?? null,
    });
  }

  /** Load the connection list. */
  async load(): Promise<ConnectionsViewState> {
    this.state = { ...this.state, phase: this.state.phase === 'ready' ? 'ready' : 'loading' };
    try {
      const response = await this.transport.list();
      this.state = {
        ...this.state,
        phase: 'ready',
        connections: sortConnectionRows(response.connections.map((connection) => this.row(connection))),
        problem: null,
      };
    } catch (error) {
      this.state = { ...this.state, phase: 'error', problem: presentProblem(error) };
    }
    return this.state;
  }

  /**
   * Create a connection, remember its transport and verify it.
   *
   * The verification runs immediately, so an operator who submitted a
   * credential reference that the pool cannot resolve is told which check
   * failed instead of discovering it on the first delivery.
   */
  async create(input: NewConnectionInput): Promise<ConnectionsViewState> {
    const validation = validateConnectionInput(input);
    if (!validation.ok) {
      this.state = {
        ...this.state,
        problem: appProblem({
          title: 'That connection form is not complete',
          message: validation.message,
          code: 'invalid_connection_input',
        }),
      };
      return this.state;
    }
    this.state = { ...this.state, busy: true, problem: null, verification: null };
    try {
      const created = await this.transport.create({
        provider: validation.input.provider,
        slug: validation.input.slug,
        display_name: validation.input.displayName,
        credential_ref: validation.input.credentialRef,
        resources: validation.input.resources,
      });
      this.preferences = writeTransportPreference(
        this.storage,
        created.id,
        validation.input.transport,
      );
      const row = this.row(created);
      this.state = {
        ...this.state,
        busy: false,
        phase: 'ready',
        connections: sortConnectionRows([...this.state.connections, row]),
      };
      return await this.verify(created.id);
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        problem: presentProblem(error),
      };
      return this.state;
    }
  }

  /**
   * Verify one connection against its current pool view.
   *
   * The only request is a read of the connection itself; no credential is
   * requested. A failure to read is reported as a failed `pool_answered` check,
   * with the pool's own sentence.
   */
  async verify(id: string): Promise<ConnectionsViewState> {
    const existing = this.state.connections.find((row) => row.id === id);
    if (!existing) {
      this.state = {
        ...this.state,
        problem: appProblem({
          title: 'That connection is not on the page',
          message: 'Reload the connections list and try again.',
          code: 'connection_not_found',
        }),
      };
      return this.state;
    }
    this.state = {
      ...this.state,
      verifyingId: id,
      busyConnectionId: id,
      problem: null,
      verification: null,
    };
    try {
      const connection = await this.transport.get(id);
      const row = this.row(connection);
      const report = verifyConnection(row, { publicOrigin: this.origin });
      this.state = {
        ...this.state,
        verifyingId: null,
        busyConnectionId: null,
        connections: sortConnectionRows(
          this.state.connections.map((item) => (item.id === id ? row : item)),
        ),
        verification: report,
      };
    } catch (error) {
      const problem = presentProblem(error);
      this.state = {
        ...this.state,
        verifyingId: null,
        busyConnectionId: null,
        verification: failedVerification(
          {
            connectionId: existing.id,
            provider: existing.providerId,
            transport: existing.transport,
          },
          problem,
        ),
      };
    }
    return this.state;
  }

  /** Rename a connection. The pool keeps the slug as the callback path. */
  async rename(id: string, displayName: string): Promise<ConnectionsViewState> {
    const name = displayName.trim();
    if (name === '' || [...name].length > 128) {
      this.state = {
        ...this.state,
        problem: appProblem({
          title: 'That name cannot be saved',
          message: 'display_name must be 1-128 characters',
          code: 'invalid_connection_name',
        }),
      };
      return this.state;
    }
    return this.update(id, { display_name: name });
  }

  /** Enable or disable a connection without deleting it. */
  async setStatus(id: string, status: 'active' | 'disabled'): Promise<ConnectionsViewState> {
    return this.update(id, { status });
  }

  /** Send one PATCH and adopt the answer. */
  private async update(
    id: string,
    input: UpdateConnectionRequest,
  ): Promise<ConnectionsViewState> {
    this.state = { ...this.state, busy: true, busyConnectionId: id, problem: null };
    try {
      const connection = await this.transport.update(id, input);
      const row = this.row(connection);
      this.state = {
        ...this.state,
        busy: false,
        busyConnectionId: null,
        connections: sortConnectionRows(
          this.state.connections.map((item) => (item.id === id ? row : item)),
        ),
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyConnectionId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /** Delete a connection. Confirmation is the page's job, not this method's. */
  async remove(id: string): Promise<ConnectionsViewState> {
    this.state = { ...this.state, busy: true, busyConnectionId: id, problem: null };
    try {
      await this.transport.remove(id);
      this.state = {
        ...this.state,
        busy: false,
        busyConnectionId: null,
        connections: this.state.connections.filter((item) => item.id !== id),
        verification:
          this.state.verification?.connectionId === id ? null : this.state.verification,
      };
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyConnectionId: null,
        problem: presentProblem(error),
      };
    }
    return this.state;
  }

  /**
   * Offer replacement of a credential.
   *
   * When the pool exposes a rotation operation the replacement is sent and the
   * connection re-verified. When it does not — as in this contract revision —
   * the page reports that plainly, naming the supported path: rotate the secret
   * the reference names, then verify again. It never echoes the value.
   */
  async replaceCredential(id: string, credentialRef: string): Promise<ConnectionsViewState> {
    const reference = credentialRef.trim();
    if (reference === '' || reference.length > 255) {
      this.state = {
        ...this.state,
        problem: appProblem({
          title: 'That credential cannot be saved',
          message: 'credential_ref must be 1-255 characters',
          code: 'invalid_credential_ref',
        }),
      };
      return this.state;
    }
    if (!this.transport.replaceCredential) {
      this.state = {
        ...this.state,
        problem: appProblem({
          title: 'This pool has no credential-replacement operation',
          message:
            'This contract revision cannot replace a credential through the API. Rotate the secret named by the connection’s credential reference in the pool environment, then verify the connection again.',
          code: 'credential_replacement_unavailable',
        }),
      };
      return this.state;
    }
    this.state = { ...this.state, busy: true, busyConnectionId: id, problem: null };
    try {
      const connection = await this.transport.replaceCredential(id, reference);
      const row = this.row(connection);
      this.state = {
        ...this.state,
        busy: false,
        busyConnectionId: null,
        connections: sortConnectionRows(
          this.state.connections.map((item) => (item.id === id ? row : item)),
        ),
      };
      return await this.verify(id);
    } catch (error) {
      this.state = {
        ...this.state,
        busy: false,
        busyConnectionId: null,
        problem: presentProblem(error),
      };
      return this.state;
    }
  }

  /**
   * Remember a Slack connection's transport.
   *
   * The contract does not carry the transport, so this is a browser preference
   * keyed by public id. It changes what the page shows and which callback URL
   * it prints; it does not write pool state.
   */
  rememberTransport(id: string, transport: ConnectionTransport): ConnectionsViewState {
    const row = this.state.connections.find((item) => item.id === id);
    if (!row?.provider || !transportBelongsTo(row.provider.provider, transport)) {
      return this.state;
    }
    this.preferences = writeTransportPreference(this.storage, id, transport);
    const next = connectionRow(
      {
        id: row.id,
        provider: row.providerId,
        slug: row.slug,
        display_name: row.displayName,
        status: row.status.value ?? '',
        managed_by: row.managedBy,
        credential_configured: row.credential.configured,
        resources: row.resources.map((resource) => ({
          resource_kind: resource.kind,
          external_id: resource.externalId,
          display_name: resource.displayName,
        })),
        created_at: row.createdAt,
      },
      { poolUrl: this.poolUrl, transport },
    );
    this.state = {
      ...this.state,
      connections: sortConnectionRows(
        this.state.connections.map((item) => (item.id === id ? next : item)),
      ),
    };
    return this.state;
  }

  /** Forget a failure. */
  clearProblem(): ConnectionsViewState {
    this.state = { ...this.state, problem: null };
    return this.state;
  }

  /** Forget a verification report. */
  clearVerification(): ConnectionsViewState {
    this.state = { ...this.state, verification: null };
    return this.state;
  }
}

/** The counts the page summarises. */
export interface ConnectionCounts {
  /** Every effective connection. */
  total: number;
  /** Delivering events. */
  active: number;
  /** Kept but no deliveries. */
  disabled: number;
  /** Configured by the environment, so read-only here. */
  environment: number;
  /** Webhook connections that need a public callback URL. */
  webhook: number;
}

/** Count rows for the summary line. */
export function connectionCounts(rows: readonly ConnectionRow[]): ConnectionCounts {
  const counts: ConnectionCounts = {
    total: rows.length,
    active: 0,
    disabled: 0,
    environment: 0,
    webhook: 0,
  };
  for (const row of rows) {
    if (row.status.value === 'active') counts.active += 1;
    if (row.status.value === 'disabled') counts.disabled += 1;
    if (row.managedBy === 'environment') counts.environment += 1;
    if (row.callback.kind === 'webhook') counts.webhook += 1;
  }
  return counts;
}

/** The one-line summary shown above the list. */
export function connectionSummary(counts: ConnectionCounts): string {
  if (counts.total === 0) return 'No connections are configured yet.';
  const parts = [`${counts.total} connection${counts.total === 1 ? '' : 's'}`];
  if (counts.active > 0) parts.push(`${counts.active} active`);
  if (counts.disabled > 0) parts.push(`${counts.disabled} disabled`);
  if (counts.environment > 0) parts.push(`${counts.environment} managed by the environment`);
  if (counts.webhook > 0) parts.push(`${counts.webhook} with a callback URL`);
  return `${parts.join(', ')}.`;
}

/** True when this pool config can load connections. */
export function canLoadConnections(
  config: { baseUrl: string | null },
  token: string | null | undefined,
): boolean {
  return poolOrigin(config.baseUrl) !== null && typeof token === 'string' && token.trim() !== '';
}
