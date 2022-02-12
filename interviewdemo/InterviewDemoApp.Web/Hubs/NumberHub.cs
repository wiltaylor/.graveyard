using Microsoft.AspNetCore.SignalR;

namespace InterviewDemoApp.Web.Hubs;

public class NumberHub: Hub<INumberClientHub>
{
    public Task SetupIntervals(int interval)
    {
        Clients.Caller.SendMessage("Setup intervals");
        return Task.CompletedTask;
    }

    public Task AddNumber(ulong number)
    {
        return Task.CompletedTask;
    }
    
    
    
    
}