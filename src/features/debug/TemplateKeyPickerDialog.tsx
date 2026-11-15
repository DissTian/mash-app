import { useCallback, useMemo, useState } from "react";
import { Box, Button, Dialog, Flex, Text, TextField } from "@radix-ui/themes";
import { MagnifyingGlassIcon } from "@radix-ui/react-icons";
import { filterTemplateKeys } from "./debugPageHelpers";

/**
 * Full template-key browser for the raw-template probe. A plain text field
 * made users guess key spellings; this lists every bundled key, filters as
 * you type (fuzzy subsequence matching) and jumps back to the current
 * selection whenever it opens.
 */
export interface TemplateThumbnailEntry {
  templateKey: string;
  /** Asset URL for the bundled PNG; `null` when the file could not be resolved. */
  src: string | null;
  /** Absolute PNG path shown under the thumbnail. */
  path: string;
}

export function TemplateKeyPickerDialog({
  open,
  onOpenChange,
  templates,
  selectedKey,
  onSelect,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  templates: TemplateThumbnailEntry[];
  selectedKey: string | null;
  onSelect: (key: string) => void;
}) {
  const [query, setQuery] = useState("");
  const entriesByKey = useMemo(() => {
    const map = new Map<string, TemplateThumbnailEntry>();
    for (const entry of templates) map.set(entry.templateKey, entry);
    return map;
  }, [templates]);
  const visibleKeys = useMemo(
    () =>
      filterTemplateKeys(
        templates.map((entry) => entry.templateKey),
        query
      ),
    [templates, query]
  );

  // Bring the current selection into view as soon as it mounts (long lists
  // otherwise open at the top and the user has to hunt for the key they
  // already picked). A callback ref runs at commit time, which is earlier and
  // more reliable than an effect once the row is inside the dialog portal.
  const scrollSelectedIntoView = useCallback((node: HTMLButtonElement | null) => {
    node?.scrollIntoView({ block: "center" });
  }, []);

  // Reopening starts from the full list so the auto-scroll can find the
  // current selection; clearing on close keeps that state change out of an
  // effect.
  const handleOpenChange = (next: boolean) => {
    if (!next) setQuery("");
    onOpenChange(next);
  };

  return (
    <Dialog.Root open={open} onOpenChange={handleOpenChange}>
      <Dialog.Content className="debug-template-picker-dialog">
        <Dialog.Title>选择模板 key</Dialog.Title>
        <Dialog.Description size="1" color="gray">
          共 {templates.length} 个模板，输入关键字可模糊搜索，单击选中。
        </Dialog.Description>
        <Box mt="2">
          <TextField.Root
            value={query}
            placeholder="搜索模板 key（支持模糊匹配）"
            aria-label="搜索模板 key"
            onChange={(event) => setQuery(event.currentTarget.value)}
          >
            <TextField.Slot>
              <MagnifyingGlassIcon height={14} width={14} />
            </TextField.Slot>
          </TextField.Root>
        </Box>
        <Box className="debug-template-picker-list" role="listbox" aria-label="模板 key">
          {visibleKeys.length === 0 ? (
            <Text size="1" color="gray">
              没有匹配的模板
            </Text>
          ) : (
            visibleKeys.map((key) => {
              const selected = key === selectedKey;
              return (
                <button
                  key={key}
                  ref={selected ? scrollSelectedIntoView : undefined}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  aria-label={key}
                  className={`debug-template-picker-item${selected ? " selected" : ""}`}
                  title={key}
                  onClick={() => {
                    onSelect(key);
                    handleOpenChange(false);
                  }}
                >
                  <span className="debug-template-picker-thumb">
                    {entriesByKey.get(key)?.src ? (
                      <img
                        src={entriesByKey.get(key)?.src ?? undefined}
                        alt=""
                        loading="lazy"
                      />
                    ) : null}
                  </span>
                  <span className="debug-template-picker-key">{key}</span>
                  <span className="debug-template-picker-path">
                    {entriesByKey.get(key)?.path ?? ""}
                  </span>
                </button>
              );
            })
          )}
        </Box>
        <Flex justify="end" mt="3">
          <Button
            type="button"
            variant="soft"
            color="gray"
            onClick={() => handleOpenChange(false)}
          >
            关闭
          </Button>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
