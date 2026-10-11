declare const pgnSafe: unique symbol;

/** Text that can be placed inside a PGN `{...}` comment. Produced only by `toPgnSafe`. */
export type PgnSafeText = string & { readonly [pgnSafe]: true };
