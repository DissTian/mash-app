import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { ApRecoveryPanel } from "../ApRecoveryPanel";

const NO_LIMITS = { gold: null, silver: null, bronze: null, copper: null, rainbow: null };

function renderPanel(
  overrides: Partial<Parameters<typeof ApRecoveryPanel>[0]> = {},
) {
  return renderWithTheme(
    <ApRecoveryPanel
      items={["gold"]}
      limits={{ ...NO_LIMITS, gold: 3 }}
      disabled
      onChangeItems={vi.fn()}
      onChangeLimit={vi.fn()}
      {...overrides}
    />,
  );
}

describe("ApRecoveryPanel", () => {
  it("shows how many limited uses are left while a run consumes them", () => {
    renderPanel({ usage: { gold: 1 } });

    expect(screen.getByText("剩 2 次")).toBeInTheDocument();
    expect(screen.getByLabelText("黄金果实剩余使用次数")).toHaveTextContent(
      "剩 2 次",
    );
  });

  it("clamps the remaining count at zero once the limit is used up", () => {
    renderPanel({ usage: { gold: 5 } });

    expect(screen.getByText("剩 0 次")).toBeInTheDocument();
  });

  it("hides the remaining badge when no run is consuming items", () => {
    renderPanel();

    expect(screen.queryByText(/剩 \d+ 次/)).toBeNull();
  });

  it("only shows the badge for items with a configured limit", () => {
    renderPanel({ limits: NO_LIMITS, usage: { gold: 1 } });

    expect(screen.queryByText(/剩 \d+ 次/)).toBeNull();
  });
});
