# Shared Model Catalog

`catalog.json` is the authoritative standard model catalog. It is editable in
Model Management and can be versioned and shared through Git. `catalog.schema.json`
provides editor validation; the application additionally validates protocol-specific
effort levels, unique IDs/names, token relationships and Provider references.

Keep each model's `id` stable once it is used by Providers. Standard definitions
contain no Provider credentials, endpoints, routes or local reference counts.
Token capacities are stored as integer counts. K/M are editor shorthand only.

Development builds read and write this repository's `models/catalog.json`.
Packaged builds initialize `<app-data>/models/catalog.json` from existing local
standards or the embedded catalog. Set `YI_LLM_MODEL_CATALOG` to an absolute file
path to use a checked-out shared catalog in either build. The current path is
shown in Model Management.

External file changes are loaded before model reads and requests. Invalid JSON,
unsupported catalog versions, duplicate definitions and changes that invalidate
Provider references are rejected, retaining the last valid database cache.
Correct the file and refresh Model Management. Reapply terminal configuration to
refresh static native model catalogs after changing model capabilities.
