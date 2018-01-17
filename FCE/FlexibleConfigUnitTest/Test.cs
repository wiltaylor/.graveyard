using AutoFixture;
using AutoFixture.AutoFakeItEasy;

namespace FlexibleConfigUnitTest
{
    public static class Test
    {
        public static IFixture Fixture => new Fixture().Customize(new AutoFakeItEasyCustomization());
    }
}
