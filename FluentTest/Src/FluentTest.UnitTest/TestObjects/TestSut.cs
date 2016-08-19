using FluentTest.UnitTest.TestObjects;

namespace FluentTest.UnitTest
{
    public class TestSut
    {
        private readonly IMockInterface _mock;
        public TestSut(IMockInterface mock)
        {
            _mock = mock;
        }

        public string AProperty { get; set; }

        public void CallMock()
        {
            _mock.CallMe();
        }
        
    }
}
