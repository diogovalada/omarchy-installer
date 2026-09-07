export interface StorageChoice {
  id:string; label:string; detail:string; sizeBytes?:number; sizeDescription?:string;
  eligible:boolean; reasons:string[];
  allocation?:{
    mode:'free'|'shrink'|'delete'|'automatic'; minimumBytes?:number; maximumBytes?:number;
    recommendedBytes?:number; currentPartitionBytes?:number|null; windowsVolume?:string|null;
  }|null;
  deletion?:{
    identifier:string; partitionLabel:string; fileSystem?:string; sizeBytes?:number;
    sizeDescription?:string; scope?:'partition'|'installation';
  }|null;
}
