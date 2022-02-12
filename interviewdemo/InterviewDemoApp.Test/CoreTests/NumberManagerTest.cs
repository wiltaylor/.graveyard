using InterviewDemoApp.Core;
using Xunit;

namespace InterviewDemoapp.Test.CoreTests;

public class NumberManagerTest
{
    private class FakeTimer : ITimer
    {
        public void SetInterval(int interval)
        {
            //Do nothing
        }

        public event EventHandler? OnTick;
        public void FakeTick()
        {
           OnTick?.Invoke(this, EventArgs.Empty); 
        }

    }

    private readonly FakeTimer _defaultTimer = new FakeTimer();

    [Fact]
    public void When_CreatingWithNegativeInterval_Should_Throw()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() =>
        {
            var sut = new NumberManager( _defaultTimer);
            sut.SetInterval(-5);
        });
    }

    [Fact]
    public void When_AddingNumbers_Should_ReturnCountsOnTimerTick()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager( timer);
        sut.SetInterval(1);

        List<KeyValuePair<ulong, ulong>>? counts = null;

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
        Assert.Contains(counts, p => p.Key == 10 & p.Value == 1u);
        Assert.Contains(counts, p => p.Key == 15 & p.Value == 1u);
        Assert.Contains(counts, p => p.Key == 2 & p.Value == 1u);
    }

    [Fact]
    public void When_Adding_MultipleNumbers_Should_IncrementCounts()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager( timer);
        sut.SetInterval(1);

        List<KeyValuePair<ulong, ulong>>? counts = null;

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
        Assert.Contains(counts!, p => p.Key == 15 & p.Value == 2u);
        Assert.Contains(counts!, p => p.Key == 2 & p.Value == 1u);
    }

    [Fact]
    public void When_CallingHalt_Should_StopTriggeringOnTick()
    {
        //Arrange
        var timer = new FakeTimer();
        var sut = new NumberManager( timer);
        sut.SetInterval(1);
        
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
        var sut = new NumberManager( timer);
        sut.SetInterval(1);
        
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
        var sut = new NumberManager( _defaultTimer);
        sut.SetInterval(1);
        
        //Act
        var result = sut.AddNumber(number);
        
        //Assert
        Assert.Equal(isFibNumber, result);
    }

    [Fact]
    public void When_CallingGetNumbers_Should_ReturnNumbersNowAndNotOnTick()
    {
        
       //Arrange 
        var sut = new NumberManager( _defaultTimer);
        sut.SetInterval(1);
        
        //Act
        sut.AddNumber(10);
        sut.AddNumber(10);
        sut.AddNumber(10);

        var result = sut.GetCounts();
        
        //Assert
        Assert.Contains(result, r => r.Key == 10 && r.Value == 3u);
    }

    [Fact]
    public void When_GettingFrequenciesOfNumbers_Should_BeInDecendingOrder()
    {
        //Arrange 
        var sut = new NumberManager( _defaultTimer);
        sut.SetInterval(1);
        
        //Act
        sut.AddNumber(1);
        sut.AddNumber(2);
        sut.AddNumber(3);

        sut.AddNumber(3);
        sut.AddNumber(2);

        sut.AddNumber(2);

        var results = sut.GetCounts();
        
        //Assert
        Assert.Equal(2u, results[0].Key);
        Assert.Equal(3u, results[1].Key);
        Assert.Equal(1u, results[2].Key);
    }
    
    
    [Fact]
    public void When_GettingFrequenciesOfNumbersFromTimer_Should_BeInDecendingOrder()
    {
        //Arrange 
        var timer = new FakeTimer();
        var sut = new NumberManager( timer);
        sut.SetInterval(1);
        
        List<KeyValuePair<ulong, ulong>> result;

        sut.OnTick += (_, args) => result = args;
        
        //Act
        sut.AddNumber(1);
        sut.AddNumber(2);
        sut.AddNumber(3);

        sut.AddNumber(3);
        sut.AddNumber(2);

        sut.AddNumber(2);
        
        timer.FakeTick();

        var results = sut.GetCounts();
        
        //Assert
        Assert.Equal(2u, results[0].Key);
        Assert.Equal(3u, results[1].Key);
        Assert.Equal(1u, results[2].Key);
    }
}