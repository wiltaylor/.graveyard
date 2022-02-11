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

        ImmutableDictionary<ulong, ulong>? counts = null;

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
        Assert.Equal(1u, counts![10]);
        Assert.Equal(1u, counts![15]);
        Assert.Equal(1u, counts![2]);
    }

    [Fact]
    public void When_Adding_MultipleNumbers_Should_IncrementCounts()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager(1, timer);

        ImmutableDictionary<ulong, ulong>? counts = null;

        sut.OnTick += (_, current) =>
        {
            counts = current;
        };
        
        //Act
        sut.AddNumber(15);
        sut.AddNumber(15);
        sut.AddNumber(2);
        
        timer.FakeTick();
        
        //Assert
        Assert.Equal(2u, counts![15]);
        Assert.Equal(1u, counts![2]); 
    }

    [Fact]
    public void When_CallingHalt_Should_StopTriggeringOnTick()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager(1, timer);
        var onTickCount = 0;

        sut.OnTick += (_,_) =>
        {
            onTickCount++;
        };

        //Act
        timer.FakeTick();
        sut.Halt();
        timer.FakeTick();

        //Assert
        Assert.Equal(1, onTickCount);
    }

    [Fact]
    public void When_CallingResume_Should_StartTriggeringOnTickAgain()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager(1, timer);
        var onTickCount = 0;

        sut.OnTick += (_,_) =>
        {
            onTickCount++;
        };

        //Act
        timer.FakeTick();
        sut.Halt();
        timer.FakeTick();
        sut.Resume();
        timer.FakeTick();

        //Assert
        Assert.Equal(2, onTickCount);
    }

    [Theory]
    [InlineData(1, true)]
    [InlineData(2, true)]
    [InlineData(3, true)]
    [InlineData(4, false)]
    [InlineData(5, true)]
    [InlineData(9897335391534825784, false)] //Check 1001 value doesn't match as per requirement
    public void When_AddingNumberWhichIsInFibSequence_Should_ReturnTrue(ulong number, bool isFibNumber)
    {
       //Arrange 
        var sut = new NumberManager(5, _defaultTimer);
        
        //Act
        var result = sut.AddNumber(number);
        
        //Assert
        Assert.Equal(isFibNumber, result);
    }

    [Fact]
    public void When_CallingGetNumbers_Should_ReturnNumbersNowAndNotOnTick()
    {
        
       //Arrange 
        var sut = new NumberManager(5, _defaultTimer);

        //Act
        sut.AddNumber(10);
        sut.AddNumber(10);
        sut.AddNumber(10);

        var result = sut.GetCounts();
        
        
        //Assert
        Assert.Equal(3u, result[10]);
    }
}