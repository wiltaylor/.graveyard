using System;
using System.Collections.Generic;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;

namespace FlexibleConfigEngine.Core.Script.Fluent
{
    public class ConfigFluent
    {
        private readonly ConfigItem _configItem;

        public ConfigFluent(ConfigItem configItem)
        {
            _configItem = configItem;
        }

        public ConfigFluent Resource(string name)
        {
            _configItem.Resource = name;
            return this;
        }

        public ConfigFluent Criteria(Func<IDataStoreReadOnly, bool> action)
        {
            _configItem.Criteria.Add(action);
            return this;
        }

        public ConfigFluent DependsOn(string name)
        {
            _configItem.Dependancy = name;
            return this;
        }

        public ConfigFluent Property(string name, string value)
        {
            if (_configItem.Properties.ContainsKey(name))
                _configItem.Properties[name] = value;
            else
                _configItem.Properties.Add(name, value);

            return this;
        }

        public ConfigFluent Row(Action<RowFluent> action)
        {
            var row = new Dictionary<string,string>();
            _configItem.RowData.Add(row);

            action(new RowFluent(row));

            return this;
        }
    }
}
