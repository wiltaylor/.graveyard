using System.Collections.Immutable;
using InterviewDemoApp.Core;
using Xunit;

namespace InterviewDemoApp.Test;

public class NumberManagerTest
{
    private class FakeTimer : ITimer
    {
        
        public event EventHandler? OnTick;
        public void FakeTick()
        {
           OnTick?.Invoke(this, EventArgs.Empty); 
        }

    }

    private readonly FakeTimer _defaultTimer = new FakeTimer();

    [Fact]
    public void When_CreatingWithPositiveTimeInterval_Should_NotThrow()
    {
        var sut = new NumberManager(5, _defaultTimer);
    }

    [Fact]
    public void When_CreatingWithNegativeInterval_Should_Throw()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() =>
        {
            var sut = new NumberManager(-5, _defaultTimer);
        });
    }

    [Fact]
    public void When_AddingNumbers_Should_ReturnCountsOnTimerTick()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager(1, timer);

        ImmutableDictionary<int, int>? counts = null;

        sut.OnTick += (_, current) =>
        {
            counts = current;
        };
        
        //Act
        sut.AddNumber(10);
        sut.AddNumber(15);
        sut.AddNumber(2);
        
        timer.FakeTick();
        
        //Assert
        Assert.Equal(1, counts![10]);
        Assert.Equal(1, counts![15]);
        Assert.Equal(1, counts![2]);
    }
}