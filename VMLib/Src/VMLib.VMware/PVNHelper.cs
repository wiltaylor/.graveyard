using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using SystemWrapper.IO;
using Newtonsoft.Json;

namespace VMLib.VMware
{
    

    public class PVNHelper : IPVNHelper
    {
        private const int PVNPartCount = 16;

        private readonly IFileWrap _file;

        public PVNHelper(IFileWrap file)
        {
            _file = file;
        }

        public string GetPVN(string networkname)
        {
            var pvnlist = new List<int>();
            var rand = new Random();
            var pvnstring = new StringBuilder();
            var pvndata = new List<PVNEntry>();
            var settingsfile = $"{Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData)}\\VMLib\\VMwarePVN.json";

            if (_file.Exists(settingsfile))
            {
                pvndata = JsonConvert.DeserializeObject<List<PVNEntry>>(
                    _file.ReadAllText(settingsfile));

                var entry = pvndata.FirstOrDefault(e => e.Name == networkname);

                if (entry != null)
                    return entry.PVN;
            }

            for (var i = 0; i < PVNPartCount; i++)
            {
                pvnstring.Append($"{rand.Next(0, 255):X2}");
                pvnstring.Append(i == 7 ? "-" : " ");
            }

            pvndata.Add(new PVNEntry {Name = networkname , PVN = pvnstring.ToString()});

            _file.WriteAllText(settingsfile, JsonConvert.SerializeObject(pvndata));

            return pvnstring.ToString();
        }

        public class PVNEntry
        {
            public string Name { get; set; }
            public string PVN { get; set; }
        }
    }
}