using System.Collections.Generic;
using Newtonsoft.Json;

namespace FlexibleConfigEngine.Core.Gather
{
    public class DataStore : IDataStore
    {
        private readonly IDictionary<string, IDictionary<string,string>> _data = new Dictionary<string, IDictionary<string, string>>();
        private readonly IDictionary<string, IDictionary<string, string>> _persistantdata = new Dictionary<string, IDictionary<string, string>>();

        public string GetPersistString(bool all=false)
        {
            return JsonConvert.SerializeObject(all ? _data : _persistantdata);
        }

        public void Write(string name, Dictionary<string,string> data, bool persist = false)
        {
            if (_data.ContainsKey(name))
                _data[name] = data;
            else
                _data.Add(name, data);

            if (persist)
            {
                if (_persistantdata.ContainsKey(name))
                    _persistantdata[name] = data;
                else
                    _persistantdata.Add(name, data);
            }
            else
            {
                if (_persistantdata.ContainsKey(name))
                    _persistantdata.Remove(name);
            }
        }

        public IDictionary<string,string> Read(string name)
        {
            return _data.ContainsKey(name) ? _data[name] : null;
        }

        public IDictionary<string, string> this[string name] => _data.ContainsKey(name) ? _data[name] : null;
    }
}
