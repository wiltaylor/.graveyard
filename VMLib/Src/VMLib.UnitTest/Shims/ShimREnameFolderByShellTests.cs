using FakeItEasy;
using NUnit.Framework;
using VMLib.Shims;

namespace VMLib.UnitTest.Shims
{
    [TestFixture]
    public class ShimRenameFolderByShellTests
    {
        [Test]
        public void RenameDirectory_RenameDirectory_ExecutesShellCommand()
        {
            var subVM = A.Fake<IVirtualMachine>();
            var sut = new ShimRenameFolderByShell(subVM);

            sut.RenameDirectory("c:\\testfolder", "c:\\newtestfoldername");

            A.CallTo(() => subVM.ExecuteCommand("c:\\windows\\system32\\cmd.exe", $"/c move /Y \"c:\\testfolder\" \"c:\\newtestfoldername\"", true, false)).MustHaveHappened();
        }
    }
}
