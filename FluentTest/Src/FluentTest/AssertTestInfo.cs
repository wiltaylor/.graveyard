namespace FluentTest
{
    public class AssertTestInfo<TContainer,TSut, TMock, TData, TResult>
    {
        private readonly TestInfo<TContainer, TSut, TMock, TData> _info;
        
        public AssertTestInfo(TestInfo<TContainer, TSut, TMock, TData> info)
        {
            _info = info;
            Result = default(TResult);
        }

        public AssertTestInfo(TestInfo<TContainer, TSut, TMock, TData> info, TResult result)
        {
            _info = info;
            Result = result;
        }

        public TData Data => _info.Data;
        public TMock Mock => _info.Mock;
        public TSut Sut => _info.Sut;
        public TResult Result { get; }
        

    }
}
