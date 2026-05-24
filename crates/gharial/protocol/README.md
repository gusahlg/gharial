# Vendored river protocols

These XML files are vendored, not fetched at build time. We pin them to a
specific upstream commit so we know exactly what protocol surface we
generate bindings against.

| file | upstream path |
| ---- | ------------- |
| `river-window-management-v1.xml` | `protocol/river-window-management-v1.xml` |
| `river-xkb-bindings-v1.xml`      | `protocol/river-xkb-bindings-v1.xml`      |
| `river-layer-shell-v1.xml`       | `protocol/river-layer-shell-v1.xml`       |

**Pinned revision:** `da8cf20fcb2c993c1c048ced4020c58d6208ef26`
(<https://github.com/riverwm/river/tree/da8cf20fcb2c993c1c048ced4020c58d6208ef26/protocol>).

## Upgrading

1. Pick a new upstream rev. Read the river changelog and any
   `<!-- VERSION X -->` comments in the XMLs.
2. Re-download all three files at that rev:
   ```sh
   REV=<sha>
   for f in river-window-management-v1.xml \
            river-xkb-bindings-v1.xml \
            river-layer-shell-v1.xml; do
     curl -sSfLo "$f" \
       "https://raw.githubusercontent.com/riverwm/river/${REV}/protocol/${f}"
   done
   ```
3. Update the rev in this file.
4. Re-run `cargo build` and address any binding diffs.
