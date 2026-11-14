import { Flex, Text } from "@radix-ui/themes";
import { ChevronRightIcon } from "@radix-ui/react-icons";
import type { ReactNode } from "react";
import { DebugHelpButton } from "./DebugHelpButton";

/**
 * Collapsible group used throughout the debug UI. Built on the native
 * `<details>` element so it needs zero React state and is naturally
 * keyboard-accessible. The chevron rotates via a CSS rule on
 * `.debug-section[open] > summary > .debug-section-chevron`.
 *
 * `help` adds a "?" button on the far right of the summary row; it opens a
 * dialog explaining what the row's options do.
 */
export function DebugSection({
  title,
  defaultOpen = false,
  badge,
  help,
  timing,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  badge?: ReactNode;
  help?: string;
  /** Debug-only measured duration for this row, in milliseconds. */
  timing?: number;
  children: ReactNode;
}) {
  return (
    <details className="debug-section" open={defaultOpen}>
      <summary className="debug-section-summary">
        <ChevronRightIcon
          className="debug-section-chevron"
          width={14}
          height={14}
        />
        <Text size="2" weight="medium">
          {title}
        </Text>
        {badge !== undefined && badge !== null && (
          <Text size="1" color="gray" className="debug-section-badge">
            {badge}
          </Text>
        )}
        {(timing != null || help) && (
          <Flex align="center" gap="2" className="debug-section-meta">
            {timing != null && (
              <Text size="1" color="gray" className="debug-section-timing">
                {timing} ms
              </Text>
            )}
            {help && <DebugHelpButton title={title} help={help} />}
          </Flex>
        )}
      </summary>
      <div className="debug-section-body">{children}</div>
    </details>
  );
}
