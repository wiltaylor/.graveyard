using InterviewDemoApp.Core;
using InterviewDemoApp.Web.Hubs;
using Microsoft.AspNetCore.SignalR;
using Moq;
using Xunit;

namespace InterviewDemoApp.Test.WebTests;

public class TestableNumberHub : NumberHub
{
    public Mock<INumberClientHub> ClientMock { get; }
    public Mock<IHubCallerClients<INumberClientHub>> ClientListMock { get; }
    public Mock<INumberManager> NumberManagerMock { get; }

    public static TestableNumberHub Default()
    {
        return new TestableNumberHub(new Mock<INumberManager>());
    }

    public TestableNumberHub(Mock<INumberManager> numberManagerMock): base(numberManagerMock.Object)
    {
        ClientMock = new Mock<INumberClientHub>();
        ClientListMock = new Mock<IHubCallerClients<INumberClientHub>>();
        NumberManagerMock = numberManagerMock;

        Clients = ClientListMock.Object;
        ClientListMock.Setup(m => m.Caller).Returns(ClientMock.Object);
    }
}

public class NumberHubTest
{
    [Fact]
    public void When_CallingSetupIntervals_Should_ReturnHowLongIntervalIsFor()
    {
        //Arrange
        var sut = TestableNumberHub.Default();

        //Act
        sut.SetupIntervals(5);
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Setup intervals for 5 seconds.")); 
    }

    [Fact]
    public void When_CallingAddNumberBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = TestableNumberHub.Default();

        //Act
        sut.AddNumber(5);
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!")); 
    }

    [Fact]
    public void When_CallingHaltBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = TestableNumberHub.Default();

        //Act
        sut.Halt();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!"));  
    }
    
    [Fact]
    public void When_CallingResumeBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = TestableNumberHub.Default();

        //Act
        sut.Resume();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!"));  
    }
    
    [Fact]
    public void When_CallingQuitBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = TestableNumberHub.Default();

        //Act
        sut.Quit();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!"));  
    }
    
    [Fact]
    public void When_CallingAddNumberAfterSetIntervals_Should_NotReturnWarningMessage()
    {
        //Arrange
        var sut = TestableNumberHub.Default();
        
        //Act
        sut.SetupIntervals(5);
        sut.AddNumber(3);
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Setup intervals for 5 seconds.")); 
        sut.ClientMock.VerifyNoOtherCalls();
    }

    [Fact]
    public void When_CallingSetIntervalAndAddingNumbers_Should_ReturnCountsWhenTimerTicks()
    {
        //Arrange
        var sut = TestableNumberHub.Default();
        var pairList = new List<KeyValuePair<ulong, ulong>>();

        //Made to match calls below in act
        pairList.Add(new KeyValuePair<ulong, ulong>(3u, 1u));
        pairList.Add(new KeyValuePair<ulong, ulong>(5u, 1u));
        
        //Act
        sut.SetupIntervals(5);
        sut.AddNumber(3);
        sut.AddNumber(5);
       
        //Simulate timer in number manager ticking.
        sut.NumberManagerMock.Raise(n => n.OnTick += null, null, pairList);
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("3:1,5:1")); 
    }

    [Fact]
    public void When_CallingHaltAfterSetInterval_Should_CallHaltOnNumberManager()
    {
        //Arrange
        var sut = TestableNumberHub.Default();
        
        //Act
        sut.SetupIntervals(5);
        sut.Halt();
        
        //Assert
        sut.NumberManagerMock.Verify(n => n.Halt());
    }

    [Fact]
    public void When_CallingResumeAfterSetInterval_Should_CallResumeOnNumberManager()
    {
        
        //Arrange
        var sut = TestableNumberHub.Default();
        
        //Act
        sut.SetupIntervals(5);
        sut.Resume();
        
        //Assert
        sut.NumberManagerMock.Verify(n => n.Resume());
    }

    [Fact]
    public void When_CallingQuitAfterSetInterval_Should_WriteCountsToClient()
    {
        //Arrange
        var sut = TestableNumberHub.Default();
        var pairList = new List<KeyValuePair<ulong, ulong>>();
        
        //Made to match calls below in act
        pairList.Add(new KeyValuePair<ulong, ulong>(3u, 1u));

        sut.NumberManagerMock.Setup(n => n.GetCounts()).Returns(pairList);
        
        //Act
        sut.SetupIntervals(5);
        sut.Quit();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("3:1")); 
    }

    [Fact]
    public void When_CallingAddNumberAfterSetInterval_Should_CallNumberManager()
    {
        //Arrange
        var sut = TestableNumberHub.Default();
        
        //Act
        sut.SetupIntervals(5);
        sut.AddNumber(3);
        
        //Assert
        sut.NumberManagerMock.Verify(n => n.AddNumber(3));
    }
}