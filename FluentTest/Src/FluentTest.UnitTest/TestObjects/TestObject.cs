using FluentTest.UnitTest.TestObjects;

namespace FluentTest.UnitTest
{
    public class TestObject
    {
        private readonly IMockInterface _mock;
        public TestObject(IMockInterface mock)
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
