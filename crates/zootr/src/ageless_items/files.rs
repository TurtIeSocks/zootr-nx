//! The files items for both ages reads, as text.

/// Every actor's sounds, by sound user.
pub const SOUNDS: &str = "SLink2/slink2.Product.111.bslnk.zs";
/// The sound banks each actor loads.
pub const BANKS: &str = "Sound/ResourceListPack/BarslistPack.Product.111.byml.zs";

/// Each Link's animation graph, whose aim picks by item ([`super::aim`]).
pub const GRAPHS: [&str; 2] = ["AS/Player_Child.root.asb.zs", "AS/Player_Adult.root.asb.zs"];

/// The files the setting reads, which the dump's cache extracts besides
/// its folders: each borrowed archive's model and events, the models of
/// each Link's own archives, whose clips a borrowed one hides, and the
/// sound files the child's Hammer sounds edit.
// Text: the extract runs before a dump is open, so no name can be read.
pub const FILES: [&str; 29] = [
    SOUNDS,
    BANKS,
    GRAPHS[0],
    GRAPHS[1],
    "Model/Player_Adult_01_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Adult_01_Animation.anim.baev.zs",
    "Model/Player_Adult_02_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Adult_02_Animation.anim.baev.zs",
    "Model/Player_Adult_02_H_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Adult_02_H_Animation.anim.baev.zs",
    "Model/Player_Adult_03_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Adult_03_Animation.anim.baev.zs",
    "Model/Player_Adult_04_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Adult_04_Animation.anim.baev.zs",
    "Model/Player_Child_01_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Child_01_Animation.anim.baev.zs",
    "Model/Player_Child_02_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Child_02_Animation.anim.baev.zs",
    "Model/Player_Child_03_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Child_03_Animation.anim.baev.zs",
    "Model/Player_Child_04_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Child_04_Animation.anim.baev.zs",
    "Model/Player_Adult_Cba_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Adult_Cba_Animation.anim.baev.zs",
    "Model/Player_Adult_Demo_Animation.anim.bfres.zs",
    "Model/Player_Dark_Animation.anim.bfres.zs",
    "Model/Player_Child_Cba_Animation.anim.bfres.zs",
    "AnimationEvent/Animation/Player_Child_Cba_Animation.anim.baev.zs",
    "Model/Player_Child_Demo_Animation.anim.bfres.zs",
];
