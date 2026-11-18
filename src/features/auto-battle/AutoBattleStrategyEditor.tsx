import { Box, Checkbox, Flex, Select, Switch, Text } from "@radix-ui/themes";
import { SectionHeading } from "../../components/common/SectionHeading";
import type { AutoBattleStrategy } from "../../types/autoBattle";
import {
  COLOR_LABELS,
  COLOR_OPTIONS,
  COMMAND_CARD_OPTIONS,
  ENEMY_TARGET_OPTIONS,
  NOBLE_PHANTASM_OPTIONS,
  SKILL_ORDER_OPTIONS,
  SKILL_TARGET_OPTIONS,
  SKILL_USE_OPTIONS,
  fullColorOrder,
  hasSelectedSkill,
  moveColor,
  toggleSkillRef,
} from "./autoBattleModel";

interface Props {
  strategy: AutoBattleStrategy;
  disabled: boolean;
  onChange: (next: AutoBattleStrategy) => void;
}

function ChoiceRow<T extends string>({
  label,
  description,
  value,
  options,
  disabled,
  onChange,
}: {
  label: string;
  description?: string;
  value: T;
  options: { value: T; label: string }[];
  disabled: boolean;
  onChange: (value: T) => void;
}) {
  return (
    <Flex align="center" justify="between" gap="4" wrap="wrap" className="auto-battle-row">
      <Flex direction="column" gap="1" className="auto-battle-row-copy">
        <Text size="2" weight="bold">
          {label}
        </Text>
        {description && (
          <Text size="1" color="gray">
            {description}
          </Text>
        )}
      </Flex>
      <Select.Root value={value} onValueChange={(next) => onChange(next as T)} disabled={disabled}>
        <Select.Trigger aria-label={label} />
        <Select.Content>
          {options.map((option) => (
            <Select.Item key={option.value} value={option.value}>
              {option.label}
            </Select.Item>
          ))}
        </Select.Content>
      </Select.Root>
    </Flex>
  );
}

function NumberChoice({
  label,
  description,
  value,
  values,
  disabled,
  onChange,
}: {
  label: string;
  description?: string;
  value: number;
  values: number[];
  disabled: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <Flex align="center" justify="between" gap="4" wrap="wrap" className="auto-battle-row">
      <Flex direction="column" gap="1" className="auto-battle-row-copy">
        <Text size="2" weight="bold">
          {label}
        </Text>
        {description && (
          <Text size="1" color="gray">
            {description}
          </Text>
        )}
      </Flex>
      <Select.Root
        value={String(value)}
        onValueChange={(next) => onChange(Number(next))}
        disabled={disabled}
      >
        <Select.Trigger aria-label={label} />
        <Select.Content>
          {values.map((option) => (
            <Select.Item key={option} value={String(option)}>
              {option}
            </Select.Item>
          ))}
        </Select.Content>
      </Select.Root>
    </Flex>
  );
}

/** Strategy form. All edits go through `onChange`; persistence stays in Rust. */
export function AutoBattleStrategyEditor({ strategy, disabled, onChange }: Props) {
  const updateSkill = (patch: Partial<AutoBattleStrategy["skill"]>) =>
    onChange({ ...strategy, skill: { ...strategy.skill, ...patch } });
  const updateSkillTarget = (patch: Partial<AutoBattleStrategy["skillTarget"]>) =>
    onChange({ ...strategy, skillTarget: { ...strategy.skillTarget, ...patch } });
  const updateNoblePhantasm = (patch: Partial<AutoBattleStrategy["noblePhantasm"]>) =>
    onChange({ ...strategy, noblePhantasm: { ...strategy.noblePhantasm, ...patch } });
  const updateCommandCard = (patch: Partial<AutoBattleStrategy["commandCard"]>) =>
    onChange({ ...strategy, commandCard: { ...strategy.commandCard, ...patch } });
  const updateEnemyTarget = (patch: Partial<AutoBattleStrategy["enemyTarget"]>) =>
    onChange({ ...strategy, enemyTarget: { ...strategy.enemyTarget, ...patch } });

  const colorOrder = fullColorOrder(strategy.commandCard.colorOrder);

  return (
    <Flex direction="column" gap="4" className="auto-battle-editor">
      <Box className="auto-battle-section">
        <SectionHeading rail stacked english="SKILLS">
          技能使用
        </SectionHeading>
        <ChoiceRow
          label="技能使用方式"
          description="只放冷却好的需要技能图标识别；识别不可用时照常尝试"
          value={strategy.skill.mode}
          options={SKILL_USE_OPTIONS}
          disabled={disabled}
          onChange={(mode) => updateSkill({ mode })}
        />
        {strategy.skill.mode === "selected" && (
          <div className="auto-battle-skill-grid" role="group" aria-label="指定技能">
            {[1, 2, 3].map((servantSlot) => (
              <div className="auto-battle-skill-row" key={servantSlot}>
                <Text size="1" color="gray">
                  从者 {servantSlot}
                </Text>
                {[1, 2, 3].map((skillSlot) => (
                  <label key={skillSlot} className="auto-battle-skill-check">
                    <Checkbox
                      checked={hasSelectedSkill(strategy, servantSlot, skillSlot)}
                      disabled={disabled}
                      onCheckedChange={() =>
                        onChange(toggleSkillRef(strategy, servantSlot, skillSlot))
                      }
                      aria-label={`从者 ${servantSlot} 技能 ${skillSlot}`}
                    />
                    <Text size="1">技能 {skillSlot}</Text>
                  </label>
                ))}
              </div>
            ))}
          </div>
        )}
        <ChoiceRow
          label="释放顺序"
          value={strategy.skill.order}
          options={SKILL_ORDER_OPTIONS}
          disabled={disabled}
          onChange={(order) => updateSkill({ order })}
        />
        <Flex align="center" justify="between" gap="4" wrap="wrap" className="auto-battle-row">
          <Flex direction="column" gap="1" className="auto-battle-row-copy">
            <Text size="2" weight="bold">
              包含御主礼装技能
            </Text>
            <Text size="1" color="gray">
              在从者技能之后使用；换人需要两个目标，默认关闭
            </Text>
          </Flex>
          <Switch
            checked={strategy.skill.includeMysticCode}
            disabled={disabled}
            onCheckedChange={(includeMysticCode) => updateSkill({ includeMysticCode })}
            aria-label="包含御主礼装技能"
          />
        </Flex>
      </Box>

      <Box className="auto-battle-section">
        <SectionHeading rail stacked english="TARGET">
          技能目标
        </SectionHeading>
        <ChoiceRow
          label="技能目标选择"
          description="技能弹出目标框时按此选择；未知从者默认给自己"
          value={strategy.skillTarget.mode}
          options={SKILL_TARGET_OPTIONS}
          disabled={disabled}
          onChange={(mode) => updateSkillTarget({ mode })}
        />
        {strategy.skillTarget.mode === "selectedSlot" && (
          <NumberChoice
            label="指定前排"
            value={strategy.skillTarget.selectedSlot ?? 1}
            values={[1, 2, 3]}
            disabled={disabled}
            onChange={(selectedSlot) => updateSkillTarget({ selectedSlot })}
          />
        )}
      </Box>

      <Box className="auto-battle-section">
        <SectionHeading rail stacked english="NOBLE PHANTASM">
          宝具使用
        </SectionHeading>
        <ChoiceRow
          label="宝具使用方式"
          description="按识别到的就绪状态插入攻击链"
          value={strategy.noblePhantasm.mode}
          options={NOBLE_PHANTASM_OPTIONS}
          disabled={disabled}
          onChange={(mode) => updateNoblePhantasm({ mode })}
        />
        <NumberChoice
          label="单回合上限"
          value={strategy.noblePhantasm.maxCount}
          values={[1, 2, 3]}
          disabled={disabled || strategy.noblePhantasm.mode === "off"}
          onChange={(maxCount) => updateNoblePhantasm({ maxCount })}
        />
      </Box>

      <Box className="auto-battle-section">
        <SectionHeading rail stacked english="COMMAND CARDS">
          卡色选择
        </SectionHeading>
        <ChoiceRow
          label="选卡方式"
          description="同色优先按下方颜色顺序尝试，凑不齐时依次退化为三色各一、左到右"
          value={strategy.commandCard.mode}
          options={COMMAND_CARD_OPTIONS}
          disabled={disabled}
          onChange={(mode) => updateCommandCard({ mode })}
        />
        <Flex direction="column" gap="2">
          <Text size="2" weight="bold">
            颜色优先顺序
          </Text>
          <Flex gap="2" wrap="wrap" className="auto-battle-color-order">
            {colorOrder.map((color, index) => (
              <Select.Root
                key={index}
                value={color}
                disabled={disabled || strategy.commandCard.mode === "leftmost"}
                onValueChange={(next) => onChange(moveColor(strategy, index, next as typeof color))}
              >
                <Select.Trigger aria-label={`第 ${index + 1} 优先颜色`} />
                <Select.Content>
                  {COLOR_OPTIONS.map((option) => (
                    <Select.Item key={option} value={option}>
                      {index + 1}. {COLOR_LABELS[option]}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            ))}
          </Flex>
        </Flex>
      </Box>

      <Box className="auto-battle-section">
        <SectionHeading rail stacked english="ENEMY">
          目标选择
        </SectionHeading>
        <ChoiceRow
          label="敌方目标选择"
          description="在攻击前选择的目标；不选择时沿用游戏默认"
          value={strategy.enemyTarget.mode}
          options={ENEMY_TARGET_OPTIONS}
          disabled={disabled}
          onChange={(mode) => updateEnemyTarget({ mode })}
        />
        {strategy.enemyTarget.mode === "selected" && (
          <NumberChoice
            label="指定敌人"
            value={strategy.enemyTarget.selected ?? 1}
            values={[1, 2, 3, 4, 5, 6]}
            disabled={disabled}
            onChange={(selected) => updateEnemyTarget({ selected })}
          />
        )}
      </Box>

      <Box className="auto-battle-section">
        <SectionHeading rail stacked english="FAILURE">
          失败处理
        </SectionHeading>
        <ChoiceRow
          label="步骤失败时"
          value={strategy.failure}
          options={[
            { value: "skipStep", label: "跳过该步继续" },
            { value: "stopRun", label: "停止本次运行" },
          ]}
          disabled={disabled}
          onChange={(failure) => onChange({ ...strategy, failure })}
        />
      </Box>
    </Flex>
  );
}
