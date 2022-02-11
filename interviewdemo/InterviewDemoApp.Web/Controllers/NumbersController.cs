using System.Net;
using System.Net.WebSockets;
using System.Text;
using System.Text.RegularExpressions;
using InterviewDemoApp.Core;
using Microsoft.AspNetCore.Mvc;

namespace InterviewDemoApp.Web.Controllers
{
    [Route("api/[controller]")]
    [ApiController]
    public class NumbersController : ControllerBase
    {
        private ITimer _timer;
        public NumbersController(ITimer timer)
        {
            _timer = timer;
        }
        
        private async Task NumberHandler(int interval, HttpContext context, WebSocket webSocket)
        {
            var numberManager = new NumberManager(interval, _timer);

            async Task SendMessage(string text)
            {
                await webSocket.SendAsync(Encoding.UTF8.GetBytes(text), WebSocketMessageType.Text, 
                    WebSocketMessageFlags.EndOfMessage, CancellationToken.None);
            }

            numberManager.OnTick += async (_, counts) =>
            {
                var message = string.Join(",", counts.Select(pair => $"{pair.Key}:{pair.Value}"));
                await SendMessage(message);
            };

            while (!webSocket.CloseStatus.HasValue)
            {
                Memory<byte> buffer = new();
                await webSocket.ReceiveAsync(buffer, CancellationToken.None);
                var text = Encoding.UTF8.GetString(buffer.Span).ToLower();

                switch (text)
                {
                    case "halt":
                        numberManager.Halt();
                        await SendMessage("Timer Halted!");
                        break; 
                    case "resume":
                        numberManager.Resume();
                        await SendMessage("Timer Resumed!");
                        break;
                    case "quit":
                        numberManager.Halt();
                        var lastCount = numberManager.GetCounts(); 
                        var message = string.Join(",", lastCount.Select(pair => $"{pair.Key}:{pair.Value}"));
                        await SendMessage(message);
                        await SendMessage("Thank you for playing!");
                        await webSocket.CloseAsync(WebSocketCloseStatus.NormalClosure, "Finished", 
                            CancellationToken.None);
                        return;
                    default:
                        if (Regex.IsMatch(text, "^[0-9]+$"))
                        {
                            numberManager.AddNumber(ulong.Parse(text));
                        }
                        else
                        {
                            await SendMessage("Unknown command!");
                        }
                        break;
                }
            }
        }
        
        [HttpGet("/ws/{interval}")]
        public async Task Get(int interval)
        {
            if (HttpContext.WebSockets.IsWebSocketRequest)
            {
                using var webSocket = await 
                    HttpContext.WebSockets.AcceptWebSocketAsync();
                await NumberHandler(interval, HttpContext, webSocket);
            }
            else
            {
                HttpContext.Response.StatusCode = (int)HttpStatusCode.BadRequest;
            }
        } 
    }
}