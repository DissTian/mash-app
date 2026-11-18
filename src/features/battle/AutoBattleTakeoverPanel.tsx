import { useEffect, useState } from "react";
import { Box, Flex, Select, Switch, Text } from "@radix-ui/themes";
import type { AutoBattleProfile } from "../../types/autoBattle";
import { listAutoBattleProfiles } from "../auto-battle/autoBattleApi";

interface Props {
  enabled: boolean;
  profileId: string | null;
  disabled?: boolean;
  onChange: (enabled: boolean, profileId: string | null) => Promise<void>;
}

/**
 * Project-level handoff switch shown in the normal-mode command editor.
 * Manual commands keep priority; the auto system only takes over once the
 * current `(battle, turn)` has no configured commands left.
 */
export function AutoBattleTakeoverPanel({ enabled, profileId, disabled = false, onChange }: Props) {
  const [profiles, setProfiles] = useState<AutoBattleProfile[] | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    listAutoBattleProfiles()
      .then((list) => {
        if (!cancelled) setProfiles(list);
      })
      .catch((reason) => {
        if (!cancelled) setError(String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const commit = async (nextEnabled: boolean, nextProfileId: string | null) => {
    setSaving(true);
    setError(null);
    try {
      await onChange(nextEnabled, nextProfileId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  const selectedProfileId =
    profiles?.find((profile) => profile.id === profileId)?.id ?? profiles?.[0]?.id ?? null;
  const locked = disabled || saving;

  return (
    <Box className="auto-battle-takeover">
      <Flex align="center" justify="between" gap="4" wrap="wrap">
        <Flex direction="column" gap="1" className="auto-battle-row-copy">
          <Text size="2" weight="bold">
            自动战斗接管
          </Text>
          <Text size="1" color="gray">
            手动指令用完后（含没有任何指令的回合）由自动战斗接管
          </Text>
        </Flex>
        <Switch
          checked={enabled}
          disabled={locked}
          aria-label="自动战斗接管"
          onCheckedChange={(next) => void commit(next, selectedProfileId)}
        />
      </Flex>
      {enabled && (
        <Flex align="center" justify="between" gap="4" wrap="wrap">
          <Text size="1" color="gray">
            使用方案
          </Text>
          <Select.Root
            value={selectedProfileId ?? ""}
            disabled={locked || profiles == null || profiles.length === 0}
            onValueChange={(next) => void commit(true, next)}
          >
            <Select.Trigger aria-label="自动战斗方案" placeholder="读取方案…" />
            <Select.Content>
              {(profiles ?? []).map((profile) => (
                <Select.Item key={profile.id} value={profile.id}>
                  {profile.name}
                  {profile.builtin ? "（内置）" : ""}
                </Select.Item>
              ))}
            </Select.Content>
          </Select.Root>
        </Flex>
      )}
      {error && (
        <Text size="1" color="red" role="alert">
          {error}
        </Text>
      )}
    </Box>
  );
}
