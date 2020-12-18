{lib, ...}:
{
  nixlib = lib;

  inherit (builtins) add addErrorContext attrNames concatLists
    deepSeq elem elemAt filter genericClosure genList getAttr
    hasAttr head isAttrs isBool isInt isList isString length
    lessThan listToAttrs pathExists readFile replaceStrings seq
    stringLength sub substring tail trace;

  stdenv = lib.stdenv;


}
