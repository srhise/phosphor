# Font provenance

Raw character-generator ROM dumps from IBM VGA hardware, obtained from
https://github.com/spacerace/romfont (`font-bin/`).

| File | Bytes | Layout | SHA-256 |
|---|---|---|---|
| `IBM_VGA_8x16.bin` | 4096 | 256 glyphs x 16 rows x 1 byte | `a8bad6fd78475a6bc2a05438c19207a9bb8c0f4f4099f60384e158cdc3eba580` |
| `IBM_VGA_8x8.bin`  | 2048 | 256 glyphs x  8 rows x 1 byte | `75c79a7e7fa423dda67ec6d6d76cec86b63f85677726368750c75b0920ddf319` |

Each byte is one row of one glyph, one bit per pixel, most significant
bit leftmost.

The upstream repository states no license, which is the usual state of
these files and not an obstacle. A bitmap font is a typeface design plus
the data that describes it, and US copyright law protects neither: 37 CFR
202.1(e) excludes typeface as such, and the raster itself carries no
authorship beyond the design. The protection that does attach to fonts
covers scalable font *programs* -- the hinting and outline code -- which a
1987 character generator has none of. These particular ROMs have been
redistributed for decades in emulators and terminal software.
