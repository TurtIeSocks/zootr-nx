/* The hook's two pieces, each linked at its offset in main, so every
   pc-relative address (adrp, b, bl) holds once they're written there:
   the cave's code, entry first, then the far code (a function's
   link_section ".far"), which rarely runs. No writable data. The build
   script defines __cave and __far. */
ENTRY(entries)
SECTIONS {
  .text __cave : {
    __hook_start = .;
    KEEP(*(.text.entry))
    *(.text .text.*)
    __code_end = .;
    *(.rodata .rodata.*)
    . = ALIGN(8);
    __hook_end = .;
  }
  .far __far : {
    *(.far)
  }
  /DISCARD/ : { *(.comment) *(.eh_frame*) *(.note*) }
}
