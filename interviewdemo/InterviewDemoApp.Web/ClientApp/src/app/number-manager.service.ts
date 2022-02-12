import { Injectable } from '@angular/core';
import * as signalR from "@microsoft/signalr";
import {environment} from "../environments/environment";
import {HubConnection} from "@microsoft/signalr";
import {from, Observable} from "rxjs";
import {tap} from "rxjs/operators";

// NumberManagerService - Handles communication with backend service via a web socket (SignalR).
@Injectable({
  providedIn: 'root'
})
export class NumberManagerService {

  connection: HubConnection;

  constructor() {
    this.connection = new signalR.HubConnectionBuilder()
      .configureLogging(signalR.LogLevel.Information)
      .withUrl( environment.apiServer + "num")
      .build();
  }

  // onMessage - Subscribe to this observable to get all messages sent from the server.
  //             These messages include the periodic count of numbers sent.
  onMessage(): Observable<string>{
    return  new Observable<string>(observer => {

      this.connection.on("SendMessage", message => {
        observer.next(message);
      });

      this.connection.onclose(()=>{
        observer.next("Connection Dropped...");
        observer.complete();
      })
    });
  }

  // setupIntervals - This method will setup the server to return all the numbers
  //                  sent to it periodically.
  connect(interval: number): Observable<void> {
    console.log("Connecting...");
    return from(
      from(this.connection.start()).pipe(
        tap(() => {
          from(this.connection.send("SetupIntervals", interval)).subscribe();
        })
      )
    );
  }

  // addNumber - Send a number to the server to count its occurrence.
  addNumber(num: number): Observable<any>{
    return from(this.connection.send("AddNumber", num));
  }

  // halt - Instruct the server to stop sending numbers every interval.
  halt() : Observable<any>{
    return from(this.connection.send("Halt"));
  }

  // resume - Instruct the server to resume sending numbers every interval.
  resume(): Observable<any>{
    return from(this.connection.send("Resume"));
  }

  // quit - Tell the server to quit number operations
  //        This will result in the server servering the connection.
  quit(): Observable<any>{
    return from(this.connection.send("Quit"));
  }
}
