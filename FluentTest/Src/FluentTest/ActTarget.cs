using System;
using FluentTest.Exceptions;

namespace FluentTest
{
    public class ActTarget<TContainer, TSut, TMock, TData>
    {
        private readonly TestInfo<TContainer,TSut, TMock, TData> _info;

        public ActTarget(TestInfo<TContainer,TSut, TMock, TData> info)
        {
            _info = info;
        }

        public AssertTarget<TContainer, TSut, TMock, TData> Act(Action<TestInfo<TContainer, TSut, TMock, TData>> predicate)
        {
            predicate(_info);

            return new AssertTarget<TContainer, TSut, TMock, TData>(new AssertTestInfo<TContainer, TSut, TMock, TData>(_info));
        }

        public void ActAndAssertThrows<TException>(Action<TestInfo<TContainer, TSut, TMock, TData>> predicate) where TException : Exception
        {
            try
            {
                predicate(_info);
            }
            catch (TException)
            {
                return;
            }
            catch (Exception e)
            {
                throw new FluentTestAssertException($"Expected exception {nameof(TException)} was not thrown. See inner exception for what was thrown instead.", e);
            }

            throw new FluentTestAssertException($"No exception was thrown at all when Exception of {nameof(TException)} was expected.");
        }
    }
}
