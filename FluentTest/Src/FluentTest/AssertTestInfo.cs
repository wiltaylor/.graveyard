namespace FluentTest
{
    public class AssertTestInfo<TContainer,TSut, TMock, TData>
    {
        private readonly TestInfo<TContainer, TSut, TMock, TData> _info;

        public AssertTestInfo(TestInfo<TContainer, TSut, TMock, TData> info)
        {
            _info = info;
        }

        public TData Data => _info.Data;
        public TMock Mock => _info.Mock;
        public TSut Sut => _info.Sut;

    }
}
