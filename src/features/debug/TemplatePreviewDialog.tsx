import { useEffect, useState } from "react";
import { Box, Button, Dialog, Flex, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { DebugTemplateImageDto } from "./debugTypes";

type PreviewState =
  | { key: string; image: DebugTemplateImageDto }
  | { key: string; error: string };

/**
 * Shows the bundled PNG behind a template key so a probe can be compared
 * against the actual art (was the template extracted from the right region,
 * is it the right scale, is the repo copy stale?). The image is fetched from
 * Rust on open — the frontend never guesses a file path.
 */
export function TemplatePreviewDialog({
  templateKey,
  onOpenChange,
}: {
  /** Template key to preview; `null` closes the dialog. */
  templateKey: string | null;
  onOpenChange: (open: boolean) => void;
}) {
  const open = templateKey !== null;
  const [state, setState] = useState<PreviewState | null>(null);

  useEffect(() => {
    if (!open || templateKey === null) return;
    let cancelled = false;
    invoke<DebugTemplateImageDto>("debug_read_template_image", { templateKey })
      .then((result) => {
        if (!cancelled) setState({ key: templateKey, image: result });
      })
      .catch((err) => {
        if (!cancelled) setState({ key: templateKey, error: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [open, templateKey]);

  // A result for a different key is stale — show the loading state until the
  // fetch for the current key lands instead of flashing the previous image.
  const active =
    open && templateKey !== null && state?.key === templateKey ? state : null;
  const loading = open && templateKey !== null && active === null;

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content className="debug-template-preview-dialog">
        <Dialog.Title>原始模板</Dialog.Title>
        <Dialog.Description size="1" color="gray">
          {templateKey}
        </Dialog.Description>
        <Flex direction="column" gap="2" mt="3">
          {loading && (
            <Text size="1" color="gray">
              加载中…
            </Text>
          )}
          {active !== null && "error" in active && (
            <Text size="1" color="red">
              {active.error}
            </Text>
          )}
          {active !== null && "image" in active && (
            <>
              <Box className="debug-template-preview-frame">
                <img
                  className="debug-template-preview-image"
                  src={active.image.dataUrl}
                  alt={`模板 ${active.image.templateKey}`}
                />
              </Box>
              <Text size="1" color="gray" className="debug-template-preview-meta">
                {active.image.path} · {active.image.sizeBytes} 字节
              </Text>
            </>
          )}
        </Flex>
        <Flex justify="end" mt="3">
          <Button
            type="button"
            variant="soft"
            color="gray"
            onClick={() => onOpenChange(false)}
          >
            关闭
          </Button>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
