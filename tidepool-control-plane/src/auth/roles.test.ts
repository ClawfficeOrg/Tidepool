import { describe, expect, it } from 'vitest';
import { ROUTES, TidepoolClient } from '@clawffice/tidepool-sdk';
import {
  CONTROLS,
  availableControls,
  canUseControl,
  controlById,
  denialReason,
  hiddenControls,
  mutatingControls,
  roleDescription,
  roleLabel,
} from './roles';
import { ROLES, type Role } from './types';

/** A client used only for its contract-derived `allows` decision. */
function decisionClient(): TidepoolClient {
  return new TidepoolClient({
    baseUrl: 'https://pool.example.com',
    fetch: async () => new Response('', { status: 204 }),
  });
}

describe('role-to-control mapping', () => {
  it('names a real contract operation for every control', () => {
    const seen = new Set<string>();
    for (const control of CONTROLS) {
      const route = ROUTES[control.operationId];
      expect(route, `control ${control.id} names ${control.operationId}`).toBeDefined();
      expect(route?.method !== 'GET').toBe(control.mutating);
      expect(route?.role ?? null).toBe(control.requiredRole);
      expect(seen.has(control.id)).toBe(false);
      seen.add(control.id);
      expect(control.label.trim()).not.toBe('');
      expect(control.description.trim()).not.toBe('');
    }
  });

  it('agrees with the SDK contract for every role and every control', () => {
    const client = decisionClient();
    for (const control of CONTROLS) {
      expect(canUseControl(null, control)).toBe(
        client.allows(control.operationId, { authenticated: false }),
      );
      for (const role of ROLES) {
        expect(
          canUseControl(role, control),
          `${control.id} for ${role}`,
        ).toBe(client.allows(control.operationId, { role }));
      }
    }
  });

  it('offers an owner every control', () => {
    expect(availableControls('owner')).toHaveLength(CONTROLS.length);
    expect(hiddenControls('owner')).toEqual([]);
  });

  it('withholds owner-only controls from admin and member', () => {
    for (const role of ['admin', 'member'] as const) {
      const hidden = hiddenControls(role);
      expect(hidden.length).toBeGreaterThan(0);
      expect(hidden.every((entry) => entry.control.requiredRole === 'owner')).toBe(true);
      expect(availableControls(role).every((control) => control.requiredRole !== 'owner')).toBe(
        true,
      );
    }
  });

  it('never offers a mutating control to a viewer', () => {
    const available = availableControls('viewer');
    expect(available.every((control) => !control.mutating)).toBe(true);
    expect(available.length).toBeGreaterThan(0);

    const hidden = hiddenControls('viewer');
    for (const control of mutatingControls()) {
      expect(available.some((candidate) => candidate.id === control.id)).toBe(false);
      expect(hidden.some((entry) => entry.control.id === control.id)).toBe(true);
    }
  });

  it('explains every hidden control without leaving a reason blank', () => {
    for (const role of [null, ...ROLES] as (Role | null)[]) {
      for (const entry of hiddenControls(role)) {
        expect(entry.reason.trim().length).toBeGreaterThan(0);
        expect(entry.reason).toContain(entry.control.label);
      }
    }
    expect(denialReason('viewer', controlById('dispatch-run')!)).toContain('read-only');
    expect(denialReason('member', controlById('invite-member')!)).toContain('owner-only');
    expect(denialReason(null, controlById('view-hosts')!)).toContain('signed-in');
  });

  it('labels roles and describes what each may do', () => {
    expect(roleLabel('owner')).toBe('Owner');
    expect(roleLabel('viewer')).toBe('Viewer');
    for (const role of ROLES) {
      expect(roleDescription(role).trim().length).toBeGreaterThan(0);
    }
    expect(roleDescription('viewer').toLowerCase()).toContain('read-only');
  });
});
