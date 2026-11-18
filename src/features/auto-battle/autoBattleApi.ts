import { invoke } from "../../tauri";
import type { AutoBattleProfile } from "../../types/autoBattle";

/** Thin wrappers over the auto-battle profile commands. */
export function listAutoBattleProfiles(): Promise<AutoBattleProfile[]> {
  return invoke<AutoBattleProfile[]>("list_auto_battle_profiles");
}

export function createAutoBattleProfile(name: string): Promise<AutoBattleProfile> {
  return invoke<AutoBattleProfile>("create_auto_battle_profile", { name });
}

export function duplicateAutoBattleProfile(
  sourceId: string,
  name: string
): Promise<AutoBattleProfile> {
  return invoke<AutoBattleProfile>("duplicate_auto_battle_profile", { sourceId, name });
}

export function saveAutoBattleProfile(profile: AutoBattleProfile): Promise<AutoBattleProfile> {
  return invoke<AutoBattleProfile>("save_auto_battle_profile", { profile });
}

export function deleteAutoBattleProfile(id: string): Promise<void> {
  return invoke<void>("delete_auto_battle_profile", { id });
}

export function resetAutoBattleProfile(id: string): Promise<AutoBattleProfile> {
  return invoke<AutoBattleProfile>("reset_auto_battle_profile", { id });
}
