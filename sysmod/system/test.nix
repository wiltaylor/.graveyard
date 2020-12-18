{lib, ...}:
{
  options.test = lib.mkOption {
    type = lib.types.str;
    default = "foooooooo";
  };
}
