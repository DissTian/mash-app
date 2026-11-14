import { describe, expect, it } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { DebugSection } from "../DebugSection";

describe("DebugSection", () => {
  it("opens the help dialog from the question button without toggling the section", async () => {
    const user = userEvent.setup();
    const { container } = renderWithTheme(
      <DebugSection title="原始模板探针" help="输入模板 key 与阈值后点「查找」。">
        <span>inner</span>
      </DebugSection>
    );
    const details = container.querySelector(
      "details.debug-section"
    ) as HTMLDetailsElement;
    expect(details.open).toBe(false);

    await user.click(
      screen.getByRole("button", { name: "原始模板探针 功能说明" })
    );

    expect(
      await screen.findByRole("dialog", { name: "原始模板探针" })
    ).toBeInTheDocument();
    expect(
      screen.getByText("输入模板 key 与阈值后点「查找」。")
    ).toBeInTheDocument();
    expect(details.open).toBe(false);

    await user.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument()
    );
  });

  it("shows the debug timing chip next to the row title", () => {
    renderWithTheme(
      <DebugSection title="识别指令卡" timing={123}>
        <span>inner</span>
      </DebugSection>
    );
    expect(screen.getByText("123 ms")).toBeInTheDocument();
  });

  it("omits the question button and timing chip when not provided", () => {
    renderWithTheme(
      <DebugSection title="无说明">
        <span>inner</span>
      </DebugSection>
    );
    expect(
      screen.queryByRole("button", { name: /功能说明/ })
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/\d+ ms/)).not.toBeInTheDocument();
  });
});
