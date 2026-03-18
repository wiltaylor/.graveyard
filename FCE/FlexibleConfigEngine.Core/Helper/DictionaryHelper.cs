using System.Collections.Generic;

namespace FlexibleConfigEngine.Core.Helper
{
    public static class DictionaryHelper
    {
        public static void Set<TK, TV>(this Dictionary<TK, TV> dict, TK key, TV value)
        {
            if (dict.ContainsKey(key))
                dict[key] = value;
            else
                dict.Add(key, value);
        }

        public static TV Get<TK, TV>(this Dictionary<TK, TV> dict, TK key)
        {
            return dict.ContainsKey(key) ? dict[key] : default(TV);
        }
    }
}
