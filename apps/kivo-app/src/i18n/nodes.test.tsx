import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import i18n from "./index";
import { withNodes } from "./nodes";

describe("withNodes", () => {
  it("places elements where the translation puts their placeholders", () => {
    const { container } = render(
      <p>{withNodes(i18n.t.bind(i18n), "home.hint", { ptt: <kbd>PTT</kbd>, type: <kbd>TYPE</kbd> })}</p>,
    );
    expect(container.textContent).toBe("Hold PTT to talk · TYPE to type");
    expect(container.querySelectorAll("kbd")).toHaveLength(2);
  });
});
