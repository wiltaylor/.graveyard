using System.Collections.Generic;

namespace FlexibleConfigEngine.Core.Script.Fluent
{
    public class RowFluent
    {
        private readonly Dictionary<string, string> _row;
        public RowFluent(Dictionary<string, string> row)
        {
            _row = row;
        }

        public RowFluent Property(string name, string value)
        {
            if (_row.ContainsKey(name))
                _row[name] = value;
            else
                _row.Add(name, value);

            return this;
        }
    }
}
