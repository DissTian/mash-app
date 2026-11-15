import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { TemplateKeyPickerDialog } from "../TemplateKeyPickerDialog";

const KEYS = [
  "battle/attack_button",
  "battle/battle_action_menu",
  "battle-result/loot",
  "shared/battle/command_seal_a",
  "support_select/row",
];

function renderPicker(
  overrides: Partial<React.ComponentProps<typeof TemplateKeyPickerDialog>> = {}
) {
  const props: React.ComponentProps<typeof TemplateKeyPickerDialog> = {
    open: true,
    onOpenChange: () => {},
    templates: KEYS.map((key) => ({
      templateKey: key,
      src: null,
      path: `/tmp/templates/${key}.png`,
    })),
    selectedKey: null,
    onSelect: () => {},
    ...overrides,
  };
  return renderWithTheme(<TemplateKeyPickerDialog {...props} />);
}

describe("TemplateKeyPickerDialog", () => {
  it("lists every template key", () => {
    renderPicker();

    for (const key of KEYS) {
      expect(screen.getByRole("option", { name: key })).toBeInTheDocument();
    }
  });

  it("renders each template's thumbnail image next to its key", () => {
    renderPicker({
      templates: [
        {
          templateKey: "battle/attack_button",
          src: "asset://attack.png",
          path: "/tmp/templates/battle/attack_button.png",
        },
        {
          templateKey: "battle-result/loot",
          src: "asset://loot.png",
          path: "/tmp/templates/battle-result/loot.png",
        },
      ],
    });

    const row = screen.getByRole("option", { name: "battle/attack_button" });
    const image = row.querySelector("img");
    expect(image).toHaveAttribute("src", "asset://attack.png");
    expect(row.textContent).toContain("battle/attack_button");
    expect(row.textContent).toContain("/tmp/templates/battle/attack_button.png");
  });

  it("filters with substring and subsequence matching", async () => {
    const user = userEvent.setup();
    renderPicker();

    const search = screen.getByRole("textbox", { name: "搜索模板 key" });
    await user.type(search, "attack");
    expect(screen.getByRole("option", { name: "battle/attack_button" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "battle-result/loot" })).toBeNull();

    await user.clear(search);
    await user.type(search, "batack");
    expect(screen.getByRole("option", { name: "battle/attack_button" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "battle-result/loot" })).toBeNull();

    await user.clear(search);
    await user.type(search, "zzz");
    expect(screen.getByText("没有匹配的模板")).toBeInTheDocument();
  });

  it("selects a key with a single click and closes", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onOpenChange = vi.fn();
    renderPicker({ onSelect, onOpenChange });

    await user.click(screen.getByRole("option", { name: "battle-result/loot" }));

    expect(onSelect).toHaveBeenCalledWith("battle-result/loot");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("scrolls the current selection into view when reopened", async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, "scrollIntoView")
      .mockImplementation(() => {});
    renderPicker({ selectedKey: "support_select/row" });

    await waitFor(() => expect(scrollSpy).toHaveBeenCalled());
    const selected = screen.getByRole("option", { name: "support_select/row" });
    expect(selected).toHaveAttribute("aria-selected", "true");
    expect(scrollSpy.mock.instances).toContain(selected);
    scrollSpy.mockRestore();
  });

  it("does not filter while closed", () => {
    renderPicker({ open: false, selectedKey: "battle/attack_button" });

    expect(screen.queryByRole("option")).toBeNull();
  });
});
