using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;

namespace FluentTest.UnitTest
{
    public class Test : TestConfigBase<Fixture>
    {
        public override Fixture BuildContainer() => (Fixture) new Fixture().Customize(new AutoFakeItEasyCustomization());
    }
}
