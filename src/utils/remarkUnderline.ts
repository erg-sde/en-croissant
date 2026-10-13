// The subset of the mdast node shape this plugin reads and writes.
interface MarkdownNode {
    type: string;
    value?: string;
    children?: MarkdownNode[];
    data?: { hName?: string };
}

const MARKER = "++";

function text(value: string): MarkdownNode {
    return { type: "text", value };
}

function underline(children: MarkdownNode[]): MarkdownNode {
    return { type: "underline", data: { hName: "u" }, children };
}

function transform(children: MarkdownNode[]): MarkdownNode[] {
    const out: MarkdownNode[] = [];
    let open: MarkdownNode[] | null = null;

    for (const child of children) {
        if (child.type !== "text" || child.value === undefined) {
            if (child.children) {
                child.children = transform(child.children);
            }
            (open ?? out).push(child);
            continue;
        }

        const parts = child.value.split(MARKER);
        for (let i = 0; i < parts.length; i++) {
            if (i > 0) {
                if (open === null) {
                    open = [];
                } else if (open.length === 0) {
                    out.push(text(MARKER + MARKER));
                    open = null;
                } else {
                    out.push(underline(open));
                    open = null;
                }
            }
            if (parts[i] !== "") {
                (open ?? out).push(text(parts[i]));
            }
        }
    }

    if (open !== null) {
        out.push(text(MARKER), ...open);
    }
    return out;
}

/**
 * Renders tiptap's underline syntax, `++text++`, as `<u>` without raw HTML. Markers pair up within
 * one paragraph or other inline container, may enclose other marks (`++**bold**++`), and are left as
 * literal text when unpaired. Code spans are not affected.
 */
export function remarkUnderline() {
    return (tree: MarkdownNode) => {
        if (tree.children) {
            tree.children = transform(tree.children);
        }
    };
}
