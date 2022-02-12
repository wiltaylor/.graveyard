using System.Dynamic;
using InterviewDemoApp.Web.Hubs;
using Microsoft.AspNetCore.SignalR;
using Moq;
using Xunit;
using Xunit.Sdk;

namespace InterviewDemoapp.Test.WebTests;

public class NumberHubTest
{
    [Fact]
    public void When_CallingSetupIntervals_Should_ReturnHowLongIntervalIsFor()
    {
        //Arrange
        var clients = new Mock<IHubCallerClients<INumberClientHub>>();
        var hubClient = new Mock<INumberClientHub>();
        var sut = new NumberHub();
        sut.Clients = clients.Object;

        clients.Setup(m => m.Caller).Returns(hubClient.Object);

        //Act
        sut.SetupIntervals(5);
        
        //Assert
        hubClient.Verify(h => h.SendMessage("Setup intervals for 5 seconds.")); 
    }

    [Fact]
    public void When_CallingAddNumberBeforeSetIntervals_Should_ReturnWarningMessage()
    {
        
        //Arrange
        var clients = new Mock<IHubCallerClients<INumberClientHub>>();
        var hubClient = new Mock<INumberClientHub>();
        var sut = new NumberHub();
        sut.Clients = clients.Object;

        clients.Setup(m => m.Caller).Returns(hubClient.Object);
        
        //Act
        sut.AddNumber(5);
        
        //Assert
        hubClient.Verify(h => h.SendMessage("Error - You need to call SetupIntervals first!")); 
    }
}