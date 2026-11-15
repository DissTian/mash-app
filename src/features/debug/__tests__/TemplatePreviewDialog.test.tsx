import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { TemplatePreviewDialog } from "../TemplatePreviewDialog";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path: string) => path),
}));

describe("TemplatePreviewDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("loads the raw template image for the current key", async () => {
    vi.mocked(invoke).mockResolvedValue({
      templateKey: "shared/battle/attack_button",
      path: "/tmp/templates/battle/attack_button.png",
      dataUrl: "data:image/png;base64,AAAA",
      sizeBytes: 2048,
    });

    renderWithTheme(
      <TemplatePreviewDialog
        templateKey="shared/battle/attack_button"
        onOpenChange={() => {}}
      />
    );

    const image = await screen.findByRole("img", {
      name: "模板 shared/battle/attack_button",
    });
    expect(image).toHaveAttribute("src", "data:image/png;base64,AAAA");
    expect(
      screen.getByText(/attack_button\.png · 2048 字节/)
    ).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("debug_read_template_image", {
      templateKey: "shared/battle/attack_button",
    });
  });

  it("surfaces a missing template as an error instead of a broken image", async () => {
    vi.mocked(invoke).mockRejectedValue("未找到模板文件: nope");

    renderWithTheme(
      <TemplatePreviewDialog templateKey="nope" onOpenChange={() => {}} />
    );

    expect(await screen.findByText("未找到模板文件: nope")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("closes through the close button", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    vi.mocked(invoke).mockResolvedValue({
      templateKey: "battle/attack_button",
      path: "/tmp/x.png",
      dataUrl: "data:image/png;base64,BBBB",
      sizeBytes: 10,
    });

    renderWithTheme(
      <TemplatePreviewDialog
        templateKey="battle/attack_button"
        onOpenChange={onOpenChange}
      />
    );

    await screen.findByRole("img");
    await user.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("does not fetch while closed", () => {
    renderWithTheme(
      <TemplatePreviewDialog templateKey={null} onOpenChange={() => {}} />
    );

    expect(invoke).not.toHaveBeenCalled();
  });
});
