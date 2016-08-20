namespace FluentTest
{
    public interface ITestInfo<TContainer, TSut, TMock, TData>
    {
        TContainer Container { get; }
        TData Data { get; set; }
        TMock Mock { get; set; }
        TSut Sut { get; set; }

        TFake Stub<TFake>(string name);
        void Stub<TFake>(string name, TFake fake);
    }
}