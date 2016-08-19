using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;

namespace FluentTest.UnitTest
{
    public class TestConfig : TestConfigBase<Fixture>
    {
        public override Fixture Container => (Fixture)new Fixture().Customize(new AutoFakeItEasyCustomization());
    }
}
