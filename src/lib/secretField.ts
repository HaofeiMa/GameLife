import type { CategoryGuides } from "./api";
import { savedCategoryGuides } from "./guides";

export const SECRET_MASK = "••••••••";

export function secretToPersist(draft: string): string | null {
  const trimmed = draft.trim();
  if (!trimmed || trimmed === SECRET_MASK) return null;
  return trimmed;
}

export function showSecretMask(present: boolean, draft: string): boolean {
  return present && !draft.trim();
}

/** Everything in `AppSettings` that reaches a `policy_versions` row. */
export interface PolicyBearingSettings {
  distractionRules: string[];
  sideProjectRules: string[];
  adminApps: string[];
  neverCaptureApps: string[];
  categoryGuides: CategoryGuides;
}

/**
 * A fingerprint of the settings the judge actually reads, for deciding whether a save
 * needs a new policy version.
 *
 * The old test was "is the 名单 tab open?", which failed in both directions: a 名单
 * edit saved from another tab wrote config.json and left the judge on the old policy,
 * while re-saving 名单 without touching anything minted another identical version
 * (nine of them on 2026-09-13). `trustedApps` / `readingApps` are deliberately absent
 * — nothing reads them any more, so changing them must not cut a version.
 */
export function policySignature(s: PolicyBearingSettings): string {
  return JSON.stringify([
    s.distractionRules,
    s.sideProjectRules,
    s.adminApps,
    s.neverCaptureApps,
    savedCategoryGuides(s.categoryGuides),
  ]);
}
