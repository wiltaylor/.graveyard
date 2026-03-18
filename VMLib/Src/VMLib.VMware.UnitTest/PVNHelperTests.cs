using System;
using System.Text.RegularExpressions;
using SystemWrapper.IO;
using FakeItEasy;
using NUnit.Framework;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class PVNHelperTests
    {

        public IPVNHelper DefaultPVNHelperFactory(IFileWrap file = null)
        {
            if (file == null)
                file = A.Fake<IFileWrap>();

            var sut = new PVNHelper(file);
            return sut;
        }

        [Test]
        public void GetPVN_GenerateAValidPVN_WillReturnValidPVN()
        {
            var sut = DefaultPVNHelperFactory();
            
            //Function is randomised so give it a good 1000 tries to make sure we don't get intermitent errors.
            for (var i = 0; i < 1000; i++)
            {
                var result = sut.GetPVN("MyNonExistingVirtualNetwork");

                Assert.That(Regex.IsMatch(result,
                    "[0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2}-[0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2} [0-9A-F]{2}"));
            }
        }

        [Test]
        public void GetPVN_PVNAlreadyExists_WillReturnPVNFromJSON()
        {
            var file = A.Fake<IFileWrap>();
            var sut = DefaultPVNHelperFactory(file: file);
            A.CallTo(() => file.Exists($"{Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData)}\\VMLib\\VMwarePVN.json")).Returns(true);
            A.CallTo(() => file.ReadAllText($"{Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData)}\\VMLib\\VMwarePVN.json")).Returns("[{\"Name\":\"ExistingNetwork\",\"PVN\":\"00 11 22 33 44 55 66 77-88 99 AA BB CC DD EE FF\"}]");

            var result = sut.GetPVN("ExistingNetwork");

            Assert.That(result == "00 11 22 33 44 55 66 77-88 99 AA BB CC DD EE FF");
        }

        [Test]
        public void GetPVN_StorePVN_WillstorePVNOnCreate()
        {
            var file = A.Fake<IFileWrap>();
            var sut = DefaultPVNHelperFactory(file: file);

            sut.GetPVN("MyPVN");

            A.CallTo(() => file.WriteAllText(null, null)).WhenArgumentsMatch((args => args[1].ToString().Contains("MyPVN"))).MustHaveHappened();

        }
    }
}