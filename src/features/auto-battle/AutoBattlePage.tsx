import { useCallback, useEffect, useState } from "react";
import { AlertDialog, Box, Button, Dialog, Flex, Text, TextField } from "@radix-ui/themes";
import { PlusIcon } from "@radix-ui/react-icons";
import { PageHeader } from "../../components/common/PageHeader";
import type { AutoBattleProfile } from "../../types/autoBattle";
import {
  createAutoBattleProfile,
  deleteAutoBattleProfile,
  duplicateAutoBattleProfile,
  listAutoBattleProfiles,
  resetAutoBattleProfile,
  saveAutoBattleProfile,
} from "./autoBattleApi";
import { AutoBattleStrategyEditor } from "./AutoBattleStrategyEditor";
import { cloneProfile, describeStrategy } from "./autoBattleModel";

interface ConfirmState {
  kind: "delete" | "reset";
  id: string;
  name: string;
}

interface AutoBattlePageProps {
  onBack: () => void;
}

export function AutoBattlePage({ onBack }: AutoBattlePageProps) {
  const [profiles, setProfiles] = useState<AutoBattleProfile[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<AutoBattleProfile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [newName, setNewName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);

  const applyList = useCallback((list: AutoBattleProfile[], preferredId?: string) => {
    setProfiles(list);
    const next = list.find((profile) => profile.id === preferredId) ?? list[0] ?? null;
    setSelectedId(next?.id ?? null);
    setDraft(next ? cloneProfile(next) : null);
  }, []);

  const load = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      applyList(await listAutoBattleProfiles());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }, [applyList]);

  useEffect(() => {
    void load();
  }, [load]);

  const selected = profiles?.find((profile) => profile.id === selectedId) ?? null;
  const dirty =
    draft != null && selected != null && JSON.stringify(draft) !== JSON.stringify(selected);

  const refresh = useCallback(
    async (preferredId?: string) => {
      const list = await listAutoBattleProfiles();
      applyList(list, preferredId);
    },
    [applyList]
  );

  const runAction = useCallback(
    async (action: () => Promise<string | undefined>, preferredId?: string) => {
      setBusy(true);
      setError(null);
      try {
        const savedId = await action();
        await refresh(savedId ?? preferredId);
      } catch (reason) {
        setError(String(reason));
      } finally {
        setBusy(false);
      }
    },
    [refresh]
  );

  const selectProfile = (profile: AutoBattleProfile) => {
    if (busy) return;
    setSelectedId(profile.id);
    setDraft(cloneProfile(profile));
    setError(null);
  };

  const save = () =>
    runAction(async () => {
      if (!draft) return undefined;
      const saved = await saveAutoBattleProfile(draft);
      return saved.id;
    });

  const duplicate = () =>
    runAction(async () => {
      if (!selected) return undefined;
      const copy = await duplicateAutoBattleProfile(selected.id, `${selected.name} 副本`);
      return copy.id;
    });

  const create = async () => {
    setBusy(true);
    setCreateError(null);
    try {
      const created = await createAutoBattleProfile(newName.trim());
      setCreateOpen(false);
      setNewName("");
      await refresh(created.id);
    } catch (reason) {
      setCreateError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const confirmAction = () => {
    if (!confirm) return;
    const { kind, id } = confirm;
    setConfirm(null);
    void runAction(async () => {
      if (kind === "delete") {
        await deleteAutoBattleProfile(id);
        return undefined;
      }
      const reset = await resetAutoBattleProfile(id);
      return reset.id;
    });
  };

  return (
    <Flex direction="column" className="battle-page workspace-page">
      <Box className="battle-topbar">
        <PageHeader title="自动战斗" english="AUTO BATTLE" onBack={onBack} backLabel="主页">
          <Text size="2" color="gray">
            策略方案
          </Text>
        </PageHeader>
      </Box>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <Box className="auto-battle-layout">
          <Flex direction="column" gap="3">
            <Flex align="center" justify="between" gap="2">
              <Text size="2" weight="bold">
                方案列表
              </Text>
              <Button size="1" onClick={() => setCreateOpen(true)} disabled={busy}>
                <PlusIcon /> 新建
              </Button>
            </Flex>
            {profiles == null ? (
              <Text size="2" color="gray">
                正在读取方案…
              </Text>
            ) : (
              <div className="auto-battle-list">
                {profiles.map((profile) => (
                  <button
                    key={profile.id}
                    type="button"
                    aria-label={`方案 ${profile.name}`}
                    aria-pressed={profile.id === selectedId}
                    className="auto-battle-profile"
                    onClick={() => selectProfile(profile)}
                  >
                    <span className="auto-battle-profile-head">
                      <strong>{profile.name}</strong>
                      {profile.builtin && <span className="auto-battle-badge">内置</span>}
                    </span>
                    <small>{describeStrategy(profile.strategy)}</small>
                  </button>
                ))}
              </div>
            )}
            {error && (
              <Text size="1" color="red" role="alert">
                {error}
              </Text>
            )}
          </Flex>

          <Flex direction="column" gap="4" className="auto-battle-detail">
            {draft && selected ? (
              <>
                <Flex align="end" justify="between" gap="3" wrap="wrap">
                  <Flex direction="column" gap="2" className="auto-battle-title-fields">
                    <TextField.Root
                      aria-label="方案名称"
                      value={draft.name}
                      disabled={busy || selected.builtin}
                      onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                    />
                    <TextField.Root
                      aria-label="方案说明"
                      value={draft.description}
                      placeholder="方案说明（可选）"
                      disabled={busy || selected.builtin}
                      onChange={(event) =>
                        setDraft({ ...draft, description: event.target.value })
                      }
                    />
                  </Flex>
                  <Flex gap="2">
                    <Button variant="soft" color="gray" onClick={duplicate} disabled={busy}>
                      复制
                    </Button>
                    {selected.builtin ? (
                      <Button
                        variant="soft"
                        color="gray"
                        disabled={busy}
                        onClick={() =>
                          setConfirm({ kind: "reset", id: selected.id, name: selected.name })
                        }
                      >
                        恢复默认
                      </Button>
                    ) : (
                      <Button
                        variant="soft"
                        color="red"
                        disabled={busy}
                        onClick={() =>
                          setConfirm({ kind: "delete", id: selected.id, name: selected.name })
                        }
                      >
                        删除
                      </Button>
                    )}
                  </Flex>
                </Flex>
                {selected.builtin && (
                  <Text size="1" color="gray">
                    基础方案只可调整策略，名称与说明固定；需要自定义名称请复制一份。
                  </Text>
                )}

                <AutoBattleStrategyEditor
                  strategy={draft.strategy}
                  disabled={busy}
                  onChange={(strategy) => setDraft({ ...draft, strategy })}
                />

                <Flex justify="end" gap="2" className="auto-battle-footer">
                  <Button
                    variant="soft"
                    color="gray"
                    disabled={busy || !dirty}
                    onClick={() => setDraft(cloneProfile(selected))}
                  >
                    放弃修改
                  </Button>
                  <Button onClick={save} disabled={busy || !dirty}>
                    保存方案
                  </Button>
                </Flex>
              </>
            ) : (
              <Text size="2" color="gray">
                还没有自动战斗方案。
              </Text>
            )}
          </Flex>
        </Box>
      </Flex>

      <Dialog.Root open={createOpen} onOpenChange={(open) => !busy && setCreateOpen(open)}>
        <Dialog.Content maxWidth="420px">
          <Dialog.Title>新建自动战斗方案</Dialog.Title>
          <Dialog.Description size="2" color="gray">
            新方案从基础方案的默认策略开始，之后可自由调整。
          </Dialog.Description>
          <TextField.Root
            aria-label="新方案名称"
            placeholder="例如：周回速刷"
            value={newName}
            disabled={busy}
            onChange={(event) => setNewName(event.target.value)}
          />
          {createError && (
            <Text size="1" color="red" role="alert">
              {createError}
            </Text>
          )}
          <Flex justify="end" gap="2" mt="4">
            <Dialog.Close>
              <Button variant="soft" color="gray" disabled={busy}>
                取消
              </Button>
            </Dialog.Close>
            <Button onClick={create} disabled={busy || !newName.trim()}>
              创建
            </Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>

      <AlertDialog.Root
        open={confirm != null}
        onOpenChange={(open) => {
          if (!open) setConfirm(null);
        }}
      >
        <AlertDialog.Content maxWidth="440px">
          <AlertDialog.Title>
            {confirm?.kind === "reset" ? `恢复「${confirm.name}」的默认策略？` : `删除方案「${confirm?.name}」？`}
          </AlertDialog.Title>
          <AlertDialog.Description size="2" color="gray">
            {confirm?.kind === "reset"
              ? "自定义过的策略会被内置基础方案覆盖。"
              : "引用该方案的项目会回退到基础方案。"}
          </AlertDialog.Description>
          <Flex justify="end" gap="2" mt="4">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button color={confirm?.kind === "delete" ? "red" : "indigo"} onClick={confirmAction}>
                确认
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}
