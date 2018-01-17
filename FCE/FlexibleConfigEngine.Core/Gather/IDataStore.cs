using System.Collections.Generic;

namespace FlexibleConfigEngine.Core.Gather
{
    public interface IDataStore : IDataStoreReadOnly
    {
        string GetPersistString(bool all = false);
        void Write(string name, Dictionary<string, string> data, bool persist = false);
    }

    public interface IDataStoreReadOnly
    {
        IDictionary<string, string> Read(string name);
        IDictionary<string, string> this[string name] { get; }
    }
}
