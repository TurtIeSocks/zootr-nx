import { expect, test } from "vitest";
import { kindOptions, kindWord } from "@/save/flag-kinds";

test("a flag type reads as a plain word, a list for an array", () => {
  expect(kindWord("Bool", false)).toBe("On/Off");
  expect(kindWord("UInt64", false)).toBe("Number");
  expect(kindWord("Vector3", false)).toBe("Position");
  expect(kindWord("WString16", false)).toBe("Text");
  expect(kindWord("IntArray", false)).toBe("Number List");
  expect(kindWord("String64Array", false)).toBe("Text List");
  expect(kindWord("Mystery", false)).toBe("Mystery");
});

test("developer tools show the raw type names", () => {
  expect(kindWord("IntArray", true)).toBe("IntArray");
  expect(kindWord("Bool", true)).toBe("Bool");
});

test("the filter merges the types that share a word", () => {
  const kinds = ["Bool", "Int", "UInt", "IntArray", "String16", "WString16"];
  expect(kindOptions(kinds, false)).toEqual([
    { word: "On/Off", kinds: ["Bool"] },
    { word: "Number", kinds: ["Int", "UInt"] },
    { word: "Number List", kinds: ["IntArray"] },
    { word: "Text", kinds: ["String16", "WString16"] },
  ]);
  expect(kindOptions(kinds, true)).toHaveLength(6);
});
