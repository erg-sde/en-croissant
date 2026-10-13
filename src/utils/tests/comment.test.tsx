import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, test } from "vitest";
import Comment from "@/components/common/Comment";

function render(comment: string): HTMLElement {
  const container = document.createElement("div");
  container.innerHTML = renderToStaticMarkup(
    <MantineProvider>
      <Comment comment={comment} />
    </MantineProvider>,
  );
  return container;
}

function eventHandlerAttributes(root: HTMLElement): string[] {
  return Array.from(root.querySelectorAll("*")).flatMap((el) =>
    el.getAttributeNames().filter((name) => name.startsWith("on")),
  );
}

describe("Comment", () => {
  test("does not create elements from raw HTML", () => {
    const root = render(
      'a <script async src="https://example.com/x.js"></script> <b ondblclick="alert(1)">bold</b> <img src=x onerror="alert(1)"> <iframe src="https://example.com"></iframe>',
    );
    expect(root.querySelector("script, b, img, iframe")).toBeNull();
    expect(eventHandlerAttributes(root)).toStrictEqual([]);
  });

  test("drops unsafe link targets", () => {
    const root = render("[x](javascript:alert(1)) and [y](https://lichess.org)");
    const links = Array.from(root.querySelectorAll("a"));
    expect(links.map((a) => a.getAttribute("href"))).toStrictEqual(["", "https://lichess.org"]);
    expect(links[1].getAttribute("target")).toBe("_blank");
  });

  test("renders tiptap underline and GFM", () => {
    const root = render("++under++ and ++**both**++ and ~~gone~~");
    const underlined = Array.from(root.querySelectorAll("u"));
    expect(underlined.map((u) => u.innerHTML)).toStrictEqual(["under", "<strong>both</strong>"]);
    expect(root.querySelector("del")?.textContent).toBe("gone");
  });

  test("leaves unpaired markers and code spans alone", () => {
    const root = render("Qh7++ wins, `a++b++c`");
    expect(root.querySelector("u")).toBeNull();
    expect(root.textContent).toContain("Qh7++ wins");
    expect(root.querySelector("code")?.textContent).toBe("a++b++c");
  });

  test("renders a GFM table", () => {
    const root = render("| a | b |\n| - | - |\n| 1 | 2 |");
    expect(root.querySelectorAll("td")).toHaveLength(2);
  });
});
