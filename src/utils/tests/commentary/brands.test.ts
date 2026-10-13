import { expect, test } from "vitest";

const SOURCES = import.meta.glob<string>(
    ["/src/**/*.{ts,tsx}", "!/src/**/tests/**", "!/src/bindings/**"],
    {
        query: "?raw",
        import: "default",
        eager: true,
    },
);

const MINTERS: Record<string, string> = {
    ValidatedNarration: "/src/utils/commentary/validate.ts",
    PgnSafeText: "/src/utils/commentary/pgnSafe.ts",
};

// The brands are a convention: only their minter may produce them. A cast anywhere else would
// let unchecked text reach the screen or a PGN file.
test("only the minters cast to the commentary brands", () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(100);
    const offenders: string[] = [];
    for (const [file, text] of Object.entries(SOURCES)) {
        for (const [brand, minter] of Object.entries(MINTERS)) {
            if (file !== minter && new RegExp(`as\\s+${brand}\\b`).test(text)) {
                offenders.push(`${file}: as ${brand}`);
            }
        }
    }
    expect(offenders).toStrictEqual([]);
});
