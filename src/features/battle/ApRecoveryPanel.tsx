import {
  Box,
  Button,
  CheckboxCards,
  Text,
  TextField,
} from "@radix-ui/themes";
import { MinusIcon, PlusIcon } from "@radix-ui/react-icons";
import { SectionHeading } from "../../components/common/SectionHeading";
import { HelpTooltip } from "../../components/common/HelpTooltip";
import type {
  BattleApRecoveryItem,
  BattleApRecoveryLimits,
} from "../../types/project";
import {
  AP_RECOVERY_HELP_TEXT,
  AP_RECOVERY_OPTIONS,
  MAX_AP_RECOVERY_LIMIT,
} from "./apRecoveryModel";

interface ApRecoveryPanelProps {
  items: BattleApRecoveryItem[];
  limits: BattleApRecoveryLimits;
  disabled: boolean;
  onChangeItems: (items: BattleApRecoveryItem[]) => void;
  onChangeLimit: (item: BattleApRecoveryItem, limit: number | null) => void;
}

/**
 * The shared "行动力恢复" settings panel.
 *
 * Battle automation and rank-up quest automation both recover AP through the
 * same runner settings, so they must expose the same controls: which items may
 * be consumed and how many of each a run may use.
 */
export function ApRecoveryPanel({
  items,
  limits,
  disabled,
  onChangeItems,
  onChangeLimit,
}: ApRecoveryPanelProps) {
  return (
    <Box className="battle-panel">
      <SectionHeading
        rail
        stacked
        english="AP RECOVERY"
        accessory={
          <HelpTooltip
            ariaLabel="行动力恢复说明"
            content={AP_RECOVERY_HELP_TEXT}
          />
        }
      >
        行动力恢复
      </SectionHeading>
      <CheckboxCards.Root
        value={items}
        className="battle-recovery-cards"
        disabled={disabled}
        onValueChange={(next) => onChangeItems(next as BattleApRecoveryItem[])}
      >
        {AP_RECOVERY_OPTIONS.map((option) => {
          const checked = items.includes(option.value);
          const limit = limits[option.value];
          return (
            <div
              className={`battle-recovery-option ${checked ? "is-selected" : ""} ${
                checked && limit != null ? "is-limited" : ""
              }`}
              key={option.value}
            >
              <CheckboxCards.Item
                value={option.value}
                className="battle-recovery-card"
                aria-label={`${option.label} ${option.recoveryLabel}`}
              >
                <img src={option.imageSrc} alt="" className="battle-recovery-image" />
                <Text size="3" weight="bold">{option.label}</Text>
                <Text size="2" weight="bold" className="battle-recovery-amount">
                  {option.recoveryLabel}
                </Text>
              </CheckboxCards.Item>
              {checked && (
                <div className="battle-recovery-limit" aria-label={`${option.label}使用限制`}>
                  <div className="battle-recovery-limit-modes">
                    <button
                      type="button"
                      className={`battle-recovery-infinity ${limit == null ? "is-active" : ""}`}
                      disabled={disabled}
                      aria-pressed={limit == null}
                      aria-label={
                        limit == null
                          ? `${option.label}无限使用`
                          : `${option.label}当前限量使用，点击改为无限`
                      }
                      onClick={() => {
                        if (limit != null) onChangeLimit(option.value, null);
                      }}
                    >
                      ∞ 无限
                    </button>
                    <button
                      type="button"
                      className={`battle-recovery-infinity ${limit != null ? "is-active" : ""}`}
                      disabled={disabled}
                      aria-pressed={limit != null}
                      aria-label={
                        limit == null
                          ? `${option.label}当前无限使用，点击设置数量`
                          : `${option.label}数量使用`
                      }
                      onClick={() => {
                        if (limit == null) onChangeLimit(option.value, 1);
                      }}
                    >
                      数量
                    </button>
                  </div>
                  {limit != null && (
                    <div className="battle-recovery-counter">
                      <Button
                        color="indigo"
                        disabled={disabled || limit <= 1}
                        aria-label={`减少${option.label}使用数量`}
                        onClick={() => onChangeLimit(option.value, limit - 1)}
                      >
                        <MinusIcon width={15} height={15} />
                      </Button>
                      <TextField.Root
                        className="battle-recovery-counter-value"
                        type="number"
                        min="1"
                        max={String(MAX_AP_RECOVERY_LIMIT)}
                        step="1"
                        variant="soft"
                        radius="none"
                        inputMode="numeric"
                        aria-label={`${option.label}使用数量`}
                        value={limit}
                        disabled={disabled}
                        onChange={(event) => {
                          const parsed = Number(event.target.value);
                          if (Number.isInteger(parsed) && parsed > 0) {
                            onChangeLimit(option.value, parsed);
                          }
                        }}
                      />
                      <Button
                        color="indigo"
                        disabled={disabled || limit >= MAX_AP_RECOVERY_LIMIT}
                        aria-label={`增加${option.label}使用数量`}
                        onClick={() => onChangeLimit(option.value, limit + 1)}
                      >
                        <PlusIcon width={15} height={15} />
                      </Button>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </CheckboxCards.Root>
    </Box>
  );
}
