using System;
using System.Collections.Generic;
using System.Linq;
using FakeItEasy;
using NUnit.Framework;
using VMLib.IOC;
using VMLib.VMware.Exceptions;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMXHelperTests
    {

        public IVMXHelper DefaultVMXHelpderFactory(string[] lines = null, IServiceDiscovery srvDiscovery = null )
        {
            if (srvDiscovery == null)
                srvDiscovery = A.Fake<IServiceDiscovery>();

            ServiceDiscovery.UnitTestInjection(srvDiscovery);

            if (lines == null)
                lines = new string[] {};

            var sut = new VMXHelper(lines);
            return sut;
        }

        public IVMNetwork DefaultVMwareNetwork(VMNetworkType type = VMNetworkType.Bridged, string macAddress = "00:00:00:00:00:00:00:00", IDictionary<string, string> customsettings = null, string isolatedNetwork = null)
        {
            if (customsettings == null)
                customsettings = new Dictionary<string, string>();

            if (!customsettings.ContainsKey("DeviceType"))
                customsettings.Add("DeviceType", "E1000");

            var fake = A.Fake<IVMNetwork>();
            A.CallTo(() => fake.Type).Returns(type);
            A.CallTo(() => fake.MACAddress).Returns(macAddress);
            A.CallTo(() => fake.CustomSettings).Returns(customsettings);
            A.CallTo(() => fake.IsolatedNetworkName).Returns(isolatedNetwork);
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
        public void WriteNetwork_AddingIsolatedNIC_WillLookUpPVN()
        {
            var srvDiscovery = A.Fake<IServiceDiscovery>();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Isolated, isolatedNetwork: "MyIsolatedNetwork");
            var pvnHelper = A.Fake<IPVNHelper>();
            var sut = DefaultVMXHelpderFactory(srvDiscovery: srvDiscovery);
            A.CallTo(() => srvDiscovery.Resolve<IPVNHelper>(A<string>.Ignored)).Returns(pvnHelper);
            A.CallTo(() => pvnHelper.GetPVN(A<string>.Ignored))
                .Returns("00 00 00 00 00 00 00 00-00 00 00 00 00 00 00 00");

            sut.WriteNetwork(network);

            A.CallTo(() => pvnHelper.GetPVN("MyIsolatedNetwork")).MustHaveHappened();
        }

        [Test]
        public void WriteNetwork_AddingIsolatedNICWithoutSettingIsolatedNetworkName_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();
            var network = DefaultVMwareNetwork(type: VMNetworkType.Isolated);

            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteNetwork(network));
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

        [Test]
        public void WriteDisk_WithFloppyDrive_WillCreateNewFloppyDrive()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "floppy0.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddingNewFloppyWithExisting_WillAddNewOne()
        {
            var lines = new[] {"floppy0.present = \"TRUE\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "floppy1.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddingNewFloppyWhen2ExistAlready_WillThrow()
        {
            var lines = new[] { "floppy0.present = \"TRUE\"", "floppy1.present = \"TRUE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);

            Assert.Throws<InvalidVMXSettingException>(() => sut.WriteDisk(disk));
        }

        [Test]
        public void WriteDisk_AddingNewFloppy_WillSetDiskTypeToFile()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);
            
            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "floppy0.fileType = \"file\""));
        }

        [Test]
        public void WriteDisk_AddingNewFloppy_WillSetFilePath()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);
            A.CallTo(() => disk.Path).Returns("c:\\myfloppy.flp");

            sut.WriteDisk(disk);

            Assert.That(() => sut.ToArray().Any(l => l == "floppy0.fileName = \"c:\\myfloppy.flp\""));
        }

        [Test]
        public void WriteDisk_UpdatingExistingDisk_WillWriteToIndexThatMAtchesVMwareIndex()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);
            A.CallTo(() => disk.CustomSettings).Returns(new Dictionary<string, string>
            {
                {"VMwareIndex", "1" }
            });

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "floppy1.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddNewCDRom_WillCreateNewEntry()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);
            
            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata0:0.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddNewCDRom_WillCreateNextFreeEntry()
        {
            var lines = new[] { "sata0:0.present = \"TRUE\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata0:1.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddNewCDRom_WillCreateEntryForBus()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata0.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddNewCDRom_WillCreateEntryForFileType()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata0:0.deviceType = \"cdrom-image\""));
        }

        [Test]
        public void WriteDisk_AddNewCDRom_WillCreateEntryForFile()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);
            A.CallTo(() => disk.Path).Returns("c:\\mycdrom.iso");
            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata0:0.fileName = \"c:\\mycdrom.iso\""));
        }

        [Test]
        public void WriteDisk_AddNewCDRom_WillCreateStartConnectedEntry()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);
            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata0:0.startConnected = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddExistingCDROM_WillOverwrite()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);
            A.CallTo(() => disk.CustomSettings).Returns(new Dictionary<string, string>
            {
                {"VMwareIndex", "1:1" }
            });

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "sata1:1.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddExistingHD_WillCreateNewEntry()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "scsi0:0.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddNewHDWhenExistingHDsExist_WillCreateNewEntry()
        {
            var lines = new[] { "scsi0:0.present = \"TRUE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "scsi0:1.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_AddExistingHD_WillOverWrite()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);
            A.CallTo(() => disk.CustomSettings).Returns(new Dictionary<string, string>
            {
                {"VMwareIndex", "1:1" }
            });

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "scsi1:1.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_NewDisk_WillCreateEntryForBusHardwareType()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "scsi0.virtualDev = \"lsisas1068\""));
        }

        [Test]
        public void WriteDisk_NewDisk_WillCreateEntryForBus()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "scsi0.present = \"TRUE\""));
        }

        [Test]
        public void WriteDisk_NewDisk_WillCreateEntryForFileName()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);
            A.CallTo(() => disk.Path).Returns("c:\\disk.vmdk");

            sut.WriteDisk(disk);

            Assert.That(sut.ToArray().Any(l => l == "scsi0:0.fileName = \"c:\\disk.vmdk\""));
        }

        [Test]
        public void WriteDisk_AddingOver60HD_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);
            A.CallTo(() => disk.Path).Returns("c:\\disk.vmdk");

            Assert.Throws<InvalidVMXSettingException>(() =>
            {
                for (var i = 0; i < 61; i++)
                    sut.WriteDisk(disk);
            });
        }

        [Test]
        public void WriteDisk_AddingOver120CDRoms_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);
            A.CallTo(() => disk.Path).Returns("c:\\disk.iso");

            Assert.Throws<InvalidVMXSettingException>(() =>
            {
                for (var i = 0; i < 121; i++)
                    sut.WriteDisk(disk);
            });
        }

        [Test]
        public void RemoveDisk_DiskNotCreateByGetDisk_WillThrow()
        {
            var sut = DefaultVMXHelpderFactory();
            var disk = A.Fake<IVMDisk>();

            Assert.Throws<InvalidVMXSettingException>(() => sut.RemoveDisk(disk));
        }

        [Test]
        public void RemoveDisk_RemoveExistingDisk_WillRemoveFromVMX()
        {
            var lines = new[] { "scsi0:0.present = \"TRUE\"", "scsi0:0.someothersetting = \"value\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.HardDisk);
            A.CallTo(() => disk.CustomSettings).Returns(new Dictionary<string, string>
            {
                {"VMwareIndex", "0:0" }
            });

            sut.RemoveDisk(disk);

            Assert.That(sut.ToArray().All(l => !l.StartsWith("scsi0:0")));
        }

        [Test]
        public void RemoveDisk_RemoveExistingCDROM_WillRemoveFromVMX()
        {
            var lines = new[] { "sata0:0.present = \"TRUE\"", "sata0:0.someothersetting = \"value\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.CDRom);
            A.CallTo(() => disk.CustomSettings).Returns(new Dictionary<string, string>
            {
                {"VMwareIndex", "0:0" }
            });

            sut.RemoveDisk(disk);

            Assert.That(sut.ToArray().All(l => !l.StartsWith("sata0:0")));
        }

        [Test]
        public void RemoveDisk_RemoveExistingFloppy_WillRemoveFromVMX()
        {
            var lines = new[] { "floppy0.present = \"TRUE\"", "floppy0.someothersetting = \"value\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => disk.Type).Returns(VMDiskType.Floppy);
            A.CallTo(() => disk.CustomSettings).Returns(new Dictionary<string, string>
            {
                {"VMwareIndex", "0" }
            });

            sut.RemoveDisk(disk);

            Assert.That(sut.ToArray().All(l => !l.StartsWith("floppy0")));
        }

        [Test]
        public void ReadDisk_WithNoDiskInVMX_WillReturnEmptyCollection()
        {
            var sut = DefaultVMXHelpderFactory();

            var result = sut.ReadDisk();

            Assert.IsEmpty(result);
        }

        [Test]
        public void ReadDisk_FloppyDiskInVMX_WillRetrunFloppyDiskObject()
        {
            var lines = new[] { "floppy0.present = \"TRUE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().Type == VMDiskType.Floppy);
        }

        [TestCase("floppy0.somesetting", "TRUE")]
        [TestCase("floppy0.someothersetting", "myvalue")]
        public void ReadDisk_FloppyDiskProperties_WillBeStoredOnFloppyObject(string setting, string value)
        {
            var lines = new[] { "floppy0.present = \"TRUE\"", $"{setting} = \"{value}\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().CustomSettings[setting] == value);
        }

        [Test]
        public void ReadDisk_FloppyHasPresentSetFalse_WillNotReturn()
        {
            var lines = new[] { "floppy0.present = \"FALSE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(!result.Any());
        }

        [Test]
        public void ReadDisk_MissingPresent_WillNotReturn()
        {
            var lines = new[] { "floppy0.someothersetting = \"TRUE\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(!result.Any());
        }

        [Test]
        public void ReadDisk_FloppyDiskPathPassed_WillReturnPathInFloppyObject()
        {
            var lines = new[] { "floppy0.present = \"TRUE\"", "floppy0.fileName = \"c:\\floppy.flp\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().Path == "c:\\floppy.flp");
        }

        [Test]
        public void ReadDisk_HDDrive_WillReturnCDRomObject()
        {
            var lines = new[] {"scsi0:0.present = \"TRUE\""};
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().Type == VMDiskType.HardDisk);
        }

        [Test]
        public void ReadDisk_HDDrive_WillReturnObjectWithDiskPath()
        {
            var lines = new[] { "scsi0:0.present = \"TRUE\"", "scsi0:0.fileName = \"MyDisk.vmdk\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().Path == "MyDisk.vmdk");
        }

        [Test]
        public void ReadDisk_HDDrive_WillHaveVMwareIndexSet()
        {
            var lines = new[] { "scsi0:0.present = \"TRUE\"", "scsi0:0.fileName = \"MyDisk.vmdk\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().CustomSettings["VMwareIndex"] == "0:0");
        }

        [TestCase("scsi0:0.MySetting", "TRUE")]
        [TestCase("scsi0:0.AnotherSetting", "TRUE")]
        public void ReadDIsk_HDDrive_WillWriteOtherVMXDataToCustomSettings(string setting, string value)
        {
            var lines = new[] { "scsi0:0.present = \"TRUE\"", $"{setting} = \"{value}\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().CustomSettings[setting] == value);
        }

        [TestCase("cdrom-image")]
        [TestCase("atapi-cdrom")]
        [TestCase("cdrom-raw")]
        public void ReadDisk_CDROM_WillReturnCDRomObject(string devicetype)
        {
            var lines = new[] { "sata0:0.present = \"TRUE\"", $"sata0:0.deviceType = \"{devicetype}\"" };
            var sut = DefaultVMXHelpderFactory(lines: lines);

            var result = sut.ReadDisk();

            Assert.That(result.First().Type == VMDiskType.CDRom);
        }
    }
}
