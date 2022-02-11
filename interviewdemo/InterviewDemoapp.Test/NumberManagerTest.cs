using InterviewDemoApp.Core;
using Xunit;

namespace InterviewDemoApp.Test;

public class NumberManagerTest
{
    [Fact]
    public void When_CreatingWithPositiveTimeInterval_Should_NotThrow()
    {
        var sut = new NumberManager(5);
    }

    [Fact]
    public void When_CreatingWithNegativeInterval_Should_Throw()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() =>
        {
            var sut = new NumberManager(-5);
        });
    }
}