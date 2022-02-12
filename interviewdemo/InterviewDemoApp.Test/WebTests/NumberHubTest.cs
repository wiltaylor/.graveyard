using InterviewDemoApp.Web.Hubs;
using Microsoft.AspNetCore.SignalR;
using Moq;
using Xunit;

namespace InterviewDemoApp.Test.WebTests;

public class TestableNumberHub : NumberHub
{
    public Mock<INumberClientHub> ClientMock { get; }
    public Mock<IHubCallerClients<INumberClientHub>> ClientListMock { get; }

    public TestableNumberHub()
    {
        ClientMock = new Mock<INumberClientHub>();
        ClientListMock = new Mock<IHubCallerClients<INumberClientHub>>();

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
        var sut = new TestableNumberHub();

        //Act
        sut.SetupIntervals(5);
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Setup intervals for 5 seconds.")); 
    }

    [Fact]
    public void When_CallingAddNumberBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = new TestableNumberHub();

        //Act
        sut.AddNumber(5);
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!")); 
    }

    [Fact]
    public void When_CallingHaltBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = new TestableNumberHub();

        //Act
        sut.Halt();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!"));  
    }
    
    [Fact]
    public void When_CallingResumeBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = new TestableNumberHub();

        //Act
        sut.Resume();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!"));  
    }
    
    [Fact]
    public void When_CallingQuitBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        //Arrange
        var sut = new TestableNumberHub();

        //Act
        sut.Quit();
        
        //Assert
        sut.ClientMock.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!"));  
    }
        
}