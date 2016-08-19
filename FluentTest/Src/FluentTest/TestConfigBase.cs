namespace FluentTest
{
    public abstract class TestConfigBase<TContainer>
    {
        public abstract TContainer Container { get; }

        public ArrangeTarget<TContainer, TSut, TMock, object> Create<TSut, TMock>()
        {
            return new ArrangeTarget<TContainer, TSut, TMock, object>(this, new TestInfo<TSut, TMock, object>());
        }

        public ArrangeTarget<TContainer, TSut, object, object> Create<TSut>()
        {
            return new ArrangeTarget<TContainer, TSut, object, object>(this, new TestInfo<TSut, object, object>());
        }

        public DataTarget<TContainer, TSut, object, TData> CreateWithData<TSut, TData>()
        {
            return new DataTarget<TContainer, TSut, object, TData>(this);
        }

        public DataTarget<TContainer, TSut, TMock, TData> CreateWithData<TSut, TMock, TData>()
        {
            return new DataTarget<TContainer, TSut, TMock, TData>(this);
        }
    }
}
