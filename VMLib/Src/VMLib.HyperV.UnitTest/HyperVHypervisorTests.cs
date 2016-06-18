using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using FakeItEasy;
using NUnit.Framework;

namespace VMLib.HyperV.UnitTest
{
    [TestFixture]
    public class HyperVHypervisorTests
    {
        public IHypervisor DefaultHyperVHypervisorFactory(IHypervisorConnectionInfo connectionInfo = null)
        {
            if (connectionInfo == null)
                connectionInfo = A.Fake<IHypervisorConnectionInfo>();

            var sut = new HyperVHypervisor();
            sut.SetConnectionSettings(connectionInfo);

            return sut;
        }

        [Test]
        public void Name_CheckNameProperty_IsVMwareWorkstation()
        {
            var sut = DefaultHyperVHypervisorFactory();

            Assert.That(sut.Name == "HyperV");
        }

        [Test]
        public void CreateNewVMInfo_CallMethod_ReturnsNewInstanceOfNewVMInfo()
        {
            var sut = DefaultHyperVHypervisorFactory();

            var result = sut.CreateNewVMInfo();

            Assert.That(result != null);
        }
    }
}
