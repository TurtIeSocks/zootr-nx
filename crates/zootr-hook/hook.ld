/* One section, entry first, no writable data. It starts at the cave's
   offset in its page, so page-relative addresses (adrp) hold once the
   code is written there. The build script defines __cave_page. */
ENTRY(entries)
SECTIONS {
  .text __cave_page : {
    __hook_start = .;
    KEEP(*(.text.entry))
    *(.text .text.*)
    __code_end = .;
    *(.rodata .rodata.*)
    . = ALIGN(8);
    __hook_end = .;
  }
  /DISCARD/ : { *(.comment) *(.eh_frame*) *(.note*) }
}
