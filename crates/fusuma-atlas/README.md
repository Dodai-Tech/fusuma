# fusuma-atlas

The types of a glyph atlas: one face at one line height, rasterised at build time to 4-bit coverage, with its glyph
table and kerning. `fusuma-fonts` (text) and `fusuma-icons` (icons) both generate them, and a renderer without a vector
rasteriser (the integer one) draws from them. `no_std`, no allocator, no floating point.
