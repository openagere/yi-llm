# Provider Icons

Copied from `cc-switch/src/icons/extracted` (local checkout), under the MIT
license in `LICENSE.cc-switch`. The upstream collection includes Lobe Icons
brand artwork. Brand names and logos remain the property of their owners.

These are local static assets. No remote image requests are made.

`brands.json` indexes all 110 icons registered by the local cc-switch icon
collection, including SVG and raster variants. Display names and search keywords
come from its icon metadata. Monochrome SVGs use that collection's default color.

Provider names are matched against this manifest's `id` and `label`, optional
`aliases`, and unambiguous brand-specific search keywords. A brand's exact name
takes precedence; otherwise the longest matching name is preferred. Latin names
require word boundaries, and matching ignores case and surrounding whitespace.

Use `aliases` to explicitly assign ambiguous search terms, such as `volcengine`
and `ark` to `huoshan`. Shared keywords and generic category tags (for example
`cloud`, `relay`, and `gateway`) do not identify a brand automatically. Existing
official-host matching takes precedence over name matching.
