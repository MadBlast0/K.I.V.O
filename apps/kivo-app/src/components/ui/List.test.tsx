import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Group, Row } from "./List";

describe("Row", () => {
  it("is a real button when clickable, and shows a chevron", () => {
    const onClick = vi.fn<() => void>();
    const { container } = render(
      <Group>
        <Row icon="mic" title="Wake word" onClick={onClick} />
      </Group>,
    );
    const button = screen.getByRole("button", { name: "Wake word" });
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
    expect(container.querySelector(".k-row__chevron")).not.toBeNull();
  });

  it("is plain content when not clickable", () => {
    render(<Row icon="mic" title="Wake word" subtitle="Listen for Hey Kivo" />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("Listen for Hey Kivo")).toBeTruthy();
  });

  it("uses the icon over a custom lead, and marks rows that have one", () => {
    const { container } = render(<Row icon="mic" lead={<span data-testid="custom" />} title="A" />);
    expect(screen.queryByTestId("custom")).toBeNull();
    expect(container.querySelector(".k-row--icon .k-row__icon svg")).not.toBeNull();
  });
});
