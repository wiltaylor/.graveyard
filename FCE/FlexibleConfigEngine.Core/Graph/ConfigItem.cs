using System;
using System.Collections.Generic;
using System.Text;
using FlexibleConfigEngine.Core.Gather;

namespace FlexibleConfigEngine.Core.Graph
{
    public class ConfigItem : IRestrictedConfigItem
    {
        public string Name { get; set; }
        public string Resource { get; set; }
        public Dictionary<string, string> Properties { get; set; } = new Dictionary<string, string>();
        public List<Func<IDataStoreReadOnly, bool>> Criteria { get; set; } = new List<Func<IDataStoreReadOnly, bool>>();
        public List<Dictionary<string, string>> RowData { get; set; } = new List<Dictionary<string, string>>();
        public string Dependancy { get; set; }
        public List<Action<IDataStoreReadOnly, IRestrictedConfigItem>> Dynamics { get; set; } = new List<Action<IDataStoreReadOnly, IRestrictedConfigItem>>();
    }

    public interface IRestrictedConfigItem
    {
        string Name { get; }
        string Resource { get; }
        string Dependancy { get; }
        Dictionary<string, string> Properties { get; set; }
        List<Dictionary<string, string>> RowData { get; set; }
    }
}
