import { useState } from "react";
import { Button, Dialog, Flex, IconButton } from "@radix-ui/themes";
import { QuestionMarkCircledIcon } from "@radix-ui/react-icons";

/**
 * "?" affordance used by every debug row/group: it opens a dialog with the
 * row's explanation. The click never reaches the surrounding row, so opening
 * help cannot collapse a `<details>` section or activate a toolbar action.
 */
export function DebugHelpButton({
  title,
  help,
  className,
}: {
  title: string;
  help: string;
  className?: string;
}) {
  const [open, setOpen] = useState(false);

  return (
    <>
      <IconButton
        type="button"
        size="1"
        variant="ghost"
        color="gray"
        className={className}
        aria-label={`${title} 功能说明`}
        title="功能说明"
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setOpen(true);
        }}
      >
        <QuestionMarkCircledIcon width={14} height={14} />
      </IconButton>
      <Dialog.Root open={open} onOpenChange={setOpen}>
        <Dialog.Content className="debug-help-dialog">
          <Dialog.Title>{title}</Dialog.Title>
          <Dialog.Description size="2" color="gray" className="debug-help-text">
            {help}
          </Dialog.Description>
          <Flex justify="end" mt="3">
            <Button
              type="button"
              variant="soft"
              color="gray"
              onClick={() => setOpen(false)}
            >
              关闭
            </Button>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}
