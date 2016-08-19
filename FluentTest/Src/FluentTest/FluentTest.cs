using System.Collections.Generic;

namespace FluentTest
{
    public class TestInfo<TSut, TMock, TData>
    {
        public TData Data { get; set; }
        public TSut Sut { get; set; }
        public TMock Mock { get; set; }
    }
}
