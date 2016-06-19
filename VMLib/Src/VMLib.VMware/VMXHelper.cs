using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using VMLib.VM;
using VMLib.VMware.Exceptions;

namespace VMLib.VMware
{
    public class VMXHelper : IVMXHelper
    {
        public const int VMWareMaxNetworkAdapters = 10;

        private readonly List<VMXSetting> _settings = new List<VMXSetting>();

        public VMXHelper(IEnumerable<string> lines)
        {
            foreach(var l in lines)
                _settings.Add(ParseLine(l));
        }

        private static VMXSetting ParseLine(string line)
        {
            var match = Regex.Match(line, "(.+) = \"(.+)\"");

            if(!match.Success)
                throw new VMXFileCorruptException($"Invalid VMX file on line: {line}");

            return new VMXSetting { Name = match.Groups[1].Value, Value = match.Groups[2].Value};
            
        }

        public void WriteVMX(string name, string value)
        {
            if (string.IsNullOrEmpty(name))
                throw new InvalidVMXSettingException("Can't pass null or empty string to WritrVMX as setting name!");

            var setting = _settings.FirstOrDefault(s => s.Name == name);

            if(setting == null)
                _settings.Add(setting = new VMXSetting { Name = name});

            setting.Value = value;

            if (value == null)
                _settings.RemoveAll(s => s.Name == name);
        }

        public void WriteNetwork(IVMNetwork network)
        {
            if(network == null)
                throw new ArgumentNullException(nameof(network));

            var index = -1;
            if (network.CustomSettings.ContainsKey("VMwareIndex"))
            {
                index = int.Parse(network.CustomSettings["VMwareIndex"]);
            } else { 
                while (index < VMWareMaxNetworkAdapters)
                {
                    index++;
                    if (_settings.Any(s => s.Name == $"ethernet{index}.present"))
                        continue;
                    break;
                }

                if(index == VMWareMaxNetworkAdapters)
                    throw new InvalidVMXSettingException($"You can only have {VMWareMaxNetworkAdapters} in a vm!");
            }

            var networktype = "bridged";

            if (network.Type == VMNetworkType.NAT)
                networktype = "nat";
            if (network.Type == VMNetworkType.HostOnly)
                networktype = "hostonly";
            if (network.Type == VMNetworkType.Isolated)
            {
                networktype = "pvn";

                if(!network.CustomSettings.ContainsKey("pvnID"))
                    throw new InvalidVMXSettingException("Expected pvnID in custom settings.");

                if(!Regex.IsMatch(network.CustomSettings["pvnID"], "[0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2}-[0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2} [0-9a-fA-F]{2}"))
                    throw new InvalidVMXSettingException("PVNID is not formed correctly.");
            }

            WriteVMX($"ethernet{index}.present", "TRUE");
            WriteVMX($"ethernet{index}.connectionType", networktype);
            WriteVMX($"ethernet{index}.virtualDev", "e1000");
            WriteVMX($"ethernet{index}.addressType", "generated");

            if(network.Type == VMNetworkType.Isolated)
                WriteVMX($"ethernet{index}.pvnID", network.CustomSettings["pvnID"]);
        }

        public void WriteDisk(IVMDisk disk)
        {
            throw new NotImplementedException();
        }

        public string[] ToArray()
        {
            return (from s in _settings
                select s.ToString()).ToArray();
        }

        public string ReadVMX(string setting)
        {
            return _settings.FirstOrDefault(s => s.Name == setting)?.Value;
        }

        public IEnumerable<IVMNetwork> ReadNetwork()
        {
            var returndata = new List<IVMNetwork>();

            for (var i = 0; i < VMWareMaxNetworkAdapters; i++)
            {
                if (_settings.All(s => s.Name != $"ethernet{i}.present")) continue;

                var nic = new VMNetwork();
                var nictype = _settings.FirstOrDefault(s => s.Name == $"ethernet{i}.connectionType");

                if(nictype == null || nictype.Value == "bridged")
                    nic.Type = VMNetworkType.Bridged;
                else switch (nictype.Value)
                {
                    case "nat":
                        nic.Type = VMNetworkType.NAT;
                        break;
                    case "hostonly":
                        nic.Type = VMNetworkType.HostOnly;
                        break;
                    case "pvn":
                        nic.Type = VMNetworkType.Isolated;
                        break;
                    default:
                        throw new InvalidVMXSettingException($"Unknown network type {nictype.Value}");
                }

                nic.MACAddress = _settings.FirstOrDefault(s => s.Name == $"ethernet{i}.generatedAddress")?.Value;

                nic.CustomSettings = new Dictionary<string, string> {{"VMwareIndex", i.ToString()}};

                foreach (var setting in _settings.Where(s => s.Name.StartsWith($"ethernet{i}")))
                {
                    nic.CustomSettings.Add(setting.Name, setting.Value);
                }

                returndata.Add(nic);
            }

            return returndata;
        }

        public void RemoveNetwork(IVMNetwork network)
        {
            if(!network.CustomSettings.ContainsKey("VMwareIndex"))
                throw new InvalidVMXSettingException("You must use network object created by ReadNetwork!");

            var index = network.CustomSettings["VMwareIndex"];

            _settings.RemoveAll(s => s.Name.StartsWith($"ethernet{index}"));
        }

        public IEnumerable<IVMDisk> ReadDisk()
        {
            throw new NotImplementedException();
        }

        public void RemoveDisk(IVMDisk disk)
        {
            throw new NotImplementedException();
        }
    }
}
