namespace FluentTest
{
    public abstract class TestConfigBase<TContainer>
    {
        public abstract TContainer BuildContainer();

        public ArrangeTarget<TContainer, TSut, TMock, object> CreateWithSutAndMock<TSut, TMock>()
        {
            return new ArrangeTarget<TContainer, TSut, TMock, object>(new TestInfo<TContainer, TSut, TMock, object>(BuildContainer()));
        }

        public ArrangeTarget<TContainer, TSut, object, object> CreateWithSut<TSut>()
        {
            return new ArrangeTarget<TContainer, TSut, object, object>(new TestInfo<TContainer, TSut, object, object>(BuildContainer()));
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
