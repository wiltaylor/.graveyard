using System.Collections.Generic;

namespace FluentTest
{
    public class TestInfo<TContainer, TSut, TMock, TData> : ITestInfo<TContainer, TSut, TMock, TData>
    {
        public TestInfo(TContainer container)
        {
            Container = container;
        }

        private readonly IDictionary<string, object> _fakes = new Dictionary<string, object>();

        public TContainer Container { get; }

        public TData Data { get; set; }
        public TSut Sut { get; set; }
        public TMock Mock { get; set; }

        public void Stub<TFake>(string name, TFake fake)
        {
            _fakes.Add(name, fake);
        }

        public TFake Stub<TFake>(string name)
        {
            return (TFake)_fakes[name];
        }
    }
}
