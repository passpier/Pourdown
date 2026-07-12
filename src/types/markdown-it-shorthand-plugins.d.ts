// None of these five markdown-it plugins ship type declarations (no `.d.ts`,
// no `types` field in package.json) and there's no `@types/...` package for
// any of them — same situation as `markdown-it-footnote.d.ts` next to this
// file. Each is a standard markdown-it plugin: a function taking the
// markdown-it instance and mutating it in place.
declare module 'markdown-it-mark' {
  export default function markPlugin(md: unknown): void;
}

declare module 'markdown-it-sub' {
  export default function subPlugin(md: unknown): void;
}

declare module 'markdown-it-sup' {
  export default function supPlugin(md: unknown): void;
}

declare module 'markdown-it-deflist' {
  export default function deflistPlugin(md: unknown): void;
}

// `markdown-it-emoji` exports three named presets (`bare`, `light`, `full`)
// rather than a single default export — see `node_modules/markdown-it-emoji/index.mjs`.
declare module 'markdown-it-emoji' {
  export const full: (md: unknown) => void;
  export const light: (md: unknown) => void;
  export const bare: (md: unknown) => void;
}
