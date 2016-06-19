using System;
using System.Collections.Generic;
using System.Linq;
using FakeItEasy;
using NUnit.Framework;
using VMLib.VMware.Exceptions;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMXHelperTests
    {

        public IVMXHelper DefaultVMXHelpderFactory(string[] lines = null )
        {
            if(lines == null)
                lines = new string[] {};

            var sut = new VMXHelper(lines);
            return sut;
        }

        public IVMNetwork DefaultVMwareNetwork(VMNetworkType type = VMNetworkType.Bridged, string macAddress = "00:00:00:00:00:00:00:00", IDictionary<string, string> customsettings = null)
        {
            if (customsettings == null)
                customsettings = new Dictionary<string, string>();

            if (!customsettings.ContainsKey("DeviceType"))
                customsettings.Add("DeviceType", "E1000");

            var fake = A.Fake<IVMNetwork>();
            A.CallTo(() => fake.Type).Returns(type);
            A.CallTo(() => fake.MACAddress).Returns(macAddress);
            A.CallTo(() => fake.CustomSettings).Returns(customsettings);
            return fake;

        }

        [Test]
        public void ToArray_WhatGoesIntoConstructor_WillComeOutToArray()
        {
            var lines = new[] {"Line1 = \"test1\"", "Line2 = \"test2\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var results = sut.ToArray();

            Assert.That(lines[0] == results[0]);
            Assert.That(lines[1] == results[1]);
        }

        [Test]
        public void Constructor_PassingInInvalidVMX_WillThrow()
        {
            var lines = new[] { "This is not a valid vmx file", "still invalid file!" };

            Assert.Throws<VMXFileCorruptException>(() => DefaultVMXHelpderFactory(lines: lines));
        }

        [Test]
        public void WriteVMX_WriteValue_SettingWillAppearInVMXText()
        {
            var sut = DefaultVMXHelpderFactory();

            sut.WriteVMX("Setting", "MyValue");

            Assert.That(sut.ToArray().Any(l => l == "Setting = \"MyValue\""));
        }

        [Test]
        public void WriteVMX_WriteNullValue_RemoveSettingFromVMX()
        {
            var lines = new[] {"Setting = \"MyValue\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);

            sut.WriteVMX("Setting", null);

            Assert.That(sut.ToArray().All( l => !l.StartsWith("Setting")));
        }

        [Test]
        public void WriteVMX_WriteExistingValue_UpdatesExistingSettingInsteadOfCreatingNewOne()
        {
            var lines = new[] { "Setting = \"MyValue\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            sut.WriteVMX("Setting", "NewValue");

            Assert.That(sut.ToArray().All(l => l != "Setting = \"MyValue\""));
            Assert.That(sut.ToArray().Any(l => l == "Setting = \"NewValue\""));
        }

        [Test]
        public void WriteVMX_WriteNullOrEmptyStringValueName_Throws()
        {
            var sut = DefaultVMXHelpderFactory();

            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteVMX(null, "anyvalue"));
            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteVMX(string.Empty, "anyvalue"));
        }

        [Test]
        public void ReadVMX_ReadingExistingValue_WillReturnValue()
        {
            var lines = new[] { "Setting = \"MyValue\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadVMX("Setting");

            Assert.That(result == "MyValue");
        }

        [Test]
        public void ReadVMX_ReadingNonExistingValue_WillReturnNull()
        {
            var lines = new[] { "Setting = \"MyValue\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadVMX("OtherSetting");

            Assert.IsNull(result);
        }

        [Test]
        public void WriteNetwork_PassingNull_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();

            Assert.Throws<ArgumentNullException>(() => sut.WriteNetwork(null));
        }

        [TestCase("ethernet0.present = \"TRUE\"")]
        [TestCase("ethernet0.connectionType = \"bridged\"")]
        [TestCase("ethernet0.virtualDev = \"e1000\"")]
        [TestCase("ethernet0.addressType = \"generated\"")]
        public void WriteNetwork_AddingBridgedNIC_WillWriteExpectedSettings(string line)
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Bridged);

            sut.WriteNetwork(network);

            Assert.That( sut.ToArray().Any(l => l == line));
        }

        [Test]
        public void WriteNetwork_AddingBridgedNIC_WillSelectedNextFreeNIC()
        {
            var lines = new[] {"ethernet0.present = \"TRUE\"", "ethernet1.present = \"FALSE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var network = DefaultVMwareNetwork();

            sut.WriteNetwork(network);

            Assert.That(sut.ToArray().Any(l => l == "ethernet2.present = \"TRUE\""));
        }

        [Test]
        public void WriteNetwork_AddingNATNIC_WillSetNetworkTypeToNAT()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.NAT);

            sut.WriteNetwork(network);

            Assert.That(sut.ToArray().Any(l => l == "ethernet0.connectionType = \"nat\""));
        }

        [Test]
        public void WriteNetwork_AddingHostOnlyNIC_WillSetNetworkTypeToHost()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.HostOnly);

            sut.WriteNetwork(network);

            Assert.That(sut.ToArray().Any(l => l == "ethernet0.connectionType = \"hostonly\""));
        }

        [Test]
        public void WriteNetwork_AddingIsolatedNIC_WillWriteNetworkTypePvn()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Isolated,
                customsettings: new Dictionary<string, string>
                {
                    { "pvnID", "00 00 00 00 00 00 00 00-00 00 00 00 00 00 00 00"}
                });

            sut.WriteNetwork(network);

            Assert.That(sut.ToArray().Any(l => l == "ethernet0.connectionType = \"pvn\""));
        }

        [Test]
        public void WriteNetwork_AddingIsolatedNICWithoutPVNID_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Isolated);

            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteNetwork(network));
        }

        [TestCase("BadString")]
        [TestCase("00 00 00 00 00 00 00 00-00 00 00 00 00 00 00")] //Missing one
        [TestCase("00 00 00 00 00 00 00 00-00 00 00 00 00 00 00 0G")] //Non Heximal
        [TestCase("00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00")] //Missing middle divider
        public void WriteNetwork_AddingIsolatedNICWithbadlyformedpvn_WillThrow(string pvn)
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Isolated,
                customsettings: new Dictionary<string, string>
                {
                    { "pvnID", pvn}
                });

            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteNetwork(network));
        }

        [Test]
        public void WriteNetwork_AddingIsolatedNIC_WillWritePvnIDToVmx()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Isolated,
                customsettings: new Dictionary<string, string>
                {
                    { "pvnID", "00 00 00 00 00 00 00 00-00 00 00 00 00 00 00 00"}
                });

            sut.WriteNetwork(network);

            Assert.That(sut.ToArray().Any(l => l == "ethernet0.pvnID = \"00 00 00 00 00 00 00 00-00 00 00 00 00 00 00 00\""));
        }

        [Test]
        public void WriteNetwork_IfCustomPropertVMwareIndexExists_OverwriteExistingNIC()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(
                customsettings: new Dictionary<string, string>
                {
                                { "VMwareIndex", "1"}
                });

            sut.WriteNetwork(network);

            Assert.That(sut.ToArray().Any(l => l == "ethernet1.present = \"TRUE\""));
        }

        [Test]
        public void WriteNetwork_AddingMoreThanMaximumNics_WillThrow()
        {
            var lines = new[]
            {
                "ethernet0.present = \"TRUE\"",
                "ethernet1.present = \"TRUE\"",
                "ethernet2.present = \"TRUE\"",
                "ethernet3.present = \"TRUE\"",
                "ethernet4.present = \"TRUE\"",
                "ethernet5.present = \"TRUE\"",
                "ethernet6.present = \"TRUE\"",
                "ethernet7.present = \"TRUE\"",
                "ethernet8.present = \"TRUE\"",
                "ethernet9.present = \"TRUE\""
            };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var network = DefaultVMwareNetwork();

            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteNetwork(network));
        }

        [Test]
        public void ReadNetwork_WithNoNetworksInVMX_WillReturnEmptyEnum()
        {
            var sut = DefaultVMXHelpderFactory();

            var result = sut.ReadNetwork();

            Assert.That(!result.Any());
        }

        [Test]
        public void ReadNetwork_WithMultipleNetworks_WillReturnANetworkObjectForEach()
        {
            var lines = new[] {"ethernet0.present = \"TRUE\"", "ethernet1.present = \"TRUE\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var results = sut.ReadNetwork();

            Assert.That(results.Count() == 2);
        }

        [TestCase("bridged", VMNetworkType.Bridged)]
        [TestCase("", VMNetworkType.Bridged)]
        [TestCase("nat", VMNetworkType.NAT)]
        [TestCase("hostonly", VMNetworkType.HostOnly)]
        [TestCase("pvn", VMNetworkType.Isolated)]
        public void ReadNetwork_ReadingNetworkTypes_WillSelectCorrectNetworkType(string vmxtype, VMNetworkType type)
        {
            var lines = new[]
            {
                "ethernet0.present = \"TRUE\"",
                $"ethernet0.connectionType = \"{vmxtype}\""
            };

            if(vmxtype == "")
                lines = new[] { "ethernet0.present = \"TRUE\"" };

            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadNetwork();

            Assert.That(result.First().Type == type);
        }

        [Test]
        public void ReadNetwork_ReadingNetworkTypeWithInvalidName_WillThrow()
        {
            var lines = new[]
            {
                "ethernet0.present = \"TRUE\"",
                "ethernet0.connectionType = \"InvalidBadType\""
            };

            var sut = DefaultVMXHelpderFactory(lines: lines);

            Assert.Throws<InvalidVMXSettingException>(() => sut.ReadNetwork());
        }

        [Test]
        public void ReadNetwork_ReadingNetworkReturnsMacAddress_WillReturnMacAddress()
        {
            var lines = new[]
            {
                "ethernet0.present = \"TRUE\"",
                "ethernet0.generatedAddress = \"00:11:22:33:44:55\""
            };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadNetwork();

            Assert.That(result.First().MACAddress == "00:11:22:33:44:55");
        }

        [Test]
        public void ReadNetwork_ReadingNetworks_WillStoreIndexInVMwareIndex()
        {
            var lines = new[] { "ethernet1.present = \"TRUE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadNetwork();

            Assert.That(result.First().CustomSettings["VMwareIndex"] == "1");
        }

        [Test]
        public void ReadNetwork_ReadingNetworks_WillStoreVMXSettingsAsCustomProperties()
        {
            var lines = new[]
{
                "ethernet0.present = \"TRUE\"",
                "ethernet0.generatedAddress = \"00:11:22:33:44:55\""
            };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadNetwork().ToArray();

            Assert.That(result.First().CustomSettings.Any(s => s.Key == "ethernet0.present" && s.Value == "TRUE"));
            Assert.That(result.First().CustomSettings.Any(s => s.Key == "ethernet0.generatedAddress" && s.Value == "00:11:22:33:44:55"));
        }

        [Test]
        public void RemoveNetwork_NoVMwareIndex_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork();

            Assert.Throws<InvalidVMXSettingException>(() => sut.RemoveNetwork(network));
        }

        [Test]
        public void RemoveNetwork_WithExistingNetworkDetails_WillRemoveNetworkFromVMX()
        {
            var lines = new[] {"ethernet1.present = \"TRUE\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var network = DefaultVMwareNetwork(customsettings: new Dictionary<string, string>
            {
                { "VMwareIndex", "1"}
            });

            sut.RemoveNetwork(network);

            Assert.That(sut.ToArray().All(l => l != "ethernet1.present = \"TRUE\""));
        }
    }
}
